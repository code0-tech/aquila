//! gRPC `ActionFileService`: lets actions upload and download files without
//! ever seeing storage URLs. Aquila brokers the bytes to and from
//! Sagittarius, streaming in both directions (nothing is buffered whole).
//!
//! Authentication: every call must carry the action's JWT in the
//! `authorization` metadata, the same token an action logs on to the
//! transfer stream with. It is verified with
//! [`ServiceConfiguration::authenticate_action`] (HS256, `sub` = the
//! action identifier, signed with that action's provisioned secret), which
//! reuses the logon path's `verify_jwt`.
//!
//! Context (`project_id` / `execution_identifier`) is passed through to
//! Sagittarius, which authorizes it: Aquila has no general registry of live
//! executions (only action-triggered ones are tracked, keyed by execution id
//! and without project or owning action), so it cannot validate it itself.

use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use bytes::{Bytes, BytesMut};
use futures::{Stream, StreamExt, stream};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tucana::{
    aquila::{
        FileDownloadRequest, FileDownloadResponse, FileUploadRequest, FileUploadResponse,
        action_file_service_server::ActionFileService, file_download_response, file_upload_request,
        file_upload_response,
    },
    sagittarius_gateway::GetFileDownloadUrlRequest,
    shared::Error as SharedError,
};

use crate::{
    authorization::authorization::extract_token,
    configuration::service::ServiceConfiguration,
    sagittarius::{
        file_service_client_impl::SagittariusFileServiceClient,
        storage_http_client_impl::{SagittariusStorageClient, StorageHttpError, UploadParams},
    },
    telemetry::errors,
    version::runtime_version,
};

/// Size of the chunks streamed to an action on download.
pub const DOWNLOAD_CHUNK_SIZE: usize = 64 * 1024;

const CODE_UNAVAILABLE: &str = "AQ-FILE-0001";
const CODE_INVALID_ARGUMENT: &str = "AQ-FILE-0002";
const CODE_TOO_LARGE: &str = "AQ-FILE-0003";
const CODE_UPSTREAM: &str = "AQ-FILE-0004";
const CODE_STREAM_ABORTED: &str = "AQ-FILE-0005";

/// What Aquila needs to reach Sagittarius; absent in static mode.
#[derive(Clone)]
pub struct FileBackend {
    pub files: SagittariusFileServiceClient,
    pub storage: SagittariusStorageClient,
}

#[derive(Clone)]
pub struct AquilaActionFileService {
    actions: ServiceConfiguration,
    backend: Option<FileBackend>,
    max_file_size: u64,
}

impl AquilaActionFileService {
    pub fn new(
        actions: ServiceConfiguration,
        backend: Option<FileBackend>,
        max_file_size: u64,
    ) -> Self {
        Self {
            actions,
            backend,
            max_file_size,
        }
    }

    /// Verifies the action's JWT and returns its identifier.
    fn authenticate<T>(&self, request: &Request<T>) -> Result<String, Status> {
        let token = extract_token(request)?;
        self.actions
            .authenticate_action(token)
            .map(str::to_string)
            .ok_or_else(|| {
                log::warn!("Rejected file transfer reason=invalid_action_token");
                Status::unauthenticated("invalid action token")
            })
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn failure(code: &str, category: &str, message: impl Into<String>) -> SharedError {
    SharedError {
        code: code.to_string(),
        category: category.to_string(),
        message: message.into(),
        timestamp: now_millis(),
        version: runtime_version().to_string(),
        ..Default::default()
    }
}

fn unavailable() -> SharedError {
    failure(
        CODE_UNAVAILABLE,
        "Unavailable",
        "file storage requires a Sagittarius connection",
    )
}

fn upload_failure(error: SharedError) -> Response<FileUploadResponse> {
    Response::new(FileUploadResponse {
        data: Some(file_upload_response::Data::Failure(error)),
    })
}

fn storage_error(error: StorageHttpError) -> SharedError {
    match error {
        StorageHttpError::Rejected { code, message } => failure(&code, "Rejected", message),
        StorageHttpError::Transport(message) => failure(CODE_UPSTREAM, "Upstream", message),
    }
}

/// Why Aquila itself stopped feeding an upload body, as opposed to
/// Sagittarius rejecting it.
enum UploadAbort {
    TooLarge { limit: u64 },
    UnexpectedMetadata,
    ClientStream(String),
}

type AbortSlot = Arc<Mutex<Option<UploadAbort>>>;

/// Turns the remainder of an upload stream into a body for the Sagittarius
/// request: chunks are forwarded as they arrive (backpressure flows through
/// to the action), counting bytes against `max` and failing the body - which
/// aborts the HTTP request - on overflow or protocol violations.
fn chunk_body(
    incoming: Streaming<FileUploadRequest>,
    max: u64,
    abort: AbortSlot,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync + 'static {
    stream::unfold(
        (incoming, 0u64, false),
        move |(mut incoming, mut received, done)| {
            let abort = abort.clone();
            async move {
                if done {
                    return None;
                }
                loop {
                    let reason = match incoming.message().await {
                        Ok(Some(FileUploadRequest {
                            data: Some(file_upload_request::Data::Chunk(chunk)),
                        })) => {
                            received += chunk.len() as u64;
                            if received > max {
                                UploadAbort::TooLarge { limit: max }
                            } else {
                                return Some((Ok(Bytes::from(chunk)), (incoming, received, false)));
                            }
                        }
                        Ok(Some(FileUploadRequest { data: None })) => continue,
                        Ok(Some(FileUploadRequest {
                            data: Some(file_upload_request::Data::Metadata(_)),
                        })) => UploadAbort::UnexpectedMetadata,
                        Ok(None) => return None,
                        Err(status) => UploadAbort::ClientStream(status.message().to_string()),
                    };
                    *abort.lock().unwrap() = Some(reason);
                    return Some((
                        Err(std::io::Error::other("upload aborted")),
                        (incoming, received, true),
                    ));
                }
            }
        },
    )
}

type DownloadStream = Pin<Box<dyn Stream<Item = Result<FileDownloadResponse, Status>> + Send>>;

fn download_failure(error: SharedError) -> Result<FileDownloadResponse, Status> {
    Ok(FileDownloadResponse {
        data: Some(file_download_response::Data::Failure(error)),
    })
}

#[tonic::async_trait]
impl ActionFileService for AquilaActionFileService {
    async fn upload(
        &self,
        request: Request<Streaming<FileUploadRequest>>,
    ) -> Result<Response<FileUploadResponse>, Status> {
        let action = self.authenticate(&request)?;
        let Some(backend) = &self.backend else {
            return Ok(upload_failure(unavailable()));
        };

        let mut incoming = request.into_inner();
        let metadata = match incoming.message().await? {
            Some(FileUploadRequest {
                data: Some(file_upload_request::Data::Metadata(metadata)),
            }) => metadata,
            _ => {
                return Ok(upload_failure(failure(
                    CODE_INVALID_ARGUMENT,
                    "InvalidArgument",
                    "the first message of an upload must be metadata",
                )));
            }
        };

        log::debug!(
            "File upload started action={} project_id={} execution={} name={:?}",
            action,
            metadata.project_id,
            metadata.execution_identifier,
            metadata.file_name
        );

        let abort: AbortSlot = Arc::default();
        let body = chunk_body(incoming, self.max_file_size, abort.clone());
        let result = backend
            .storage
            .upload(
                UploadParams {
                    file_name: metadata.file_name,
                    content_type: metadata.content_type,
                    project_id: metadata.project_id,
                    execution_identifier: metadata.execution_identifier,
                    expires_at: metadata.expires_at,
                },
                body,
            )
            .await;

        // An abort on our side is the root cause of whatever the HTTP call reported.
        if let Some(reason) = abort.lock().unwrap().take() {
            return Ok(upload_failure(match reason {
                UploadAbort::TooLarge { limit } => failure(
                    CODE_TOO_LARGE,
                    "PayloadTooLarge",
                    format!("file exceeds the maximum allowed size of {limit} bytes"),
                ),
                UploadAbort::UnexpectedMetadata => failure(
                    CODE_INVALID_ARGUMENT,
                    "InvalidArgument",
                    "metadata may only be sent as the first message",
                ),
                UploadAbort::ClientStream(message) => failure(
                    CODE_STREAM_ABORTED,
                    "Aborted",
                    format!("upload stream failed: {message}"),
                ),
            }));
        }

        match result {
            Ok(file) => Ok(Response::new(FileUploadResponse {
                data: Some(file_upload_response::Data::File(file)),
            })),
            Err(error) => {
                if let StorageHttpError::Transport(message) = &error {
                    errors::record_message("dependency", "sagittarius.file.upload", message, "");
                }
                Ok(upload_failure(storage_error(error)))
            }
        }
    }

    type DownloadStream = DownloadStream;

    async fn download(
        &self,
        request: Request<FileDownloadRequest>,
    ) -> Result<Response<Self::DownloadStream>, Status> {
        let action = self.authenticate(&request)?;
        let Some(backend) = self.backend.clone() else {
            return Ok(Response::new(Box::pin(stream::iter([download_failure(
                unavailable(),
            )]))));
        };

        let request = request.into_inner();
        log::debug!(
            "File download started action={} project_id={} execution={}",
            action,
            request.project_id,
            request.execution_identifier
        );

        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tokio::spawn(async move {
            if let Err(error) = stream_download(&backend, request, &tx).await {
                let _ = tx.send(download_failure(error)).await;
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

/// Resolves the signed URL, then forwards metadata and the body in
/// [`DOWNLOAD_CHUNK_SIZE`] chunks. An `Err` is sent to the action as the
/// `failure` variant; a closed channel (action went away) ends quietly.
async fn stream_download(
    backend: &FileBackend,
    request: FileDownloadRequest,
    tx: &tokio::sync::mpsc::Sender<Result<FileDownloadResponse, Status>>,
) -> Result<(), SharedError> {
    let resolved = backend
        .files
        .get_file_download_url(GetFileDownloadUrlRequest {
            id: request.id,
            project_id: request.project_id,
            execution_identifier: request.execution_identifier,
        })
        .await
        .map_err(|status| {
            failure(
                CODE_UPSTREAM,
                "Upstream",
                format!("could not reach Sagittarius: {}", status.message()),
            )
        })?;

    if let Some(error) = resolved.error {
        return Err(failure(CODE_UPSTREAM, "Rejected", error.message));
    }
    let file = resolved.file.ok_or_else(|| {
        failure(
            CODE_UPSTREAM,
            "Upstream",
            "Sagittarius returned no file metadata",
        )
    })?;
    if resolved.url.is_empty() {
        return Err(failure(
            CODE_UPSTREAM,
            "Upstream",
            "Sagittarius returned no download url",
        ));
    }

    let url = backend
        .storage
        .resolve_download_url(&resolved.url)
        .map_err(|message| failure(CODE_UPSTREAM, "Upstream", message))?;
    // Request the bytes before announcing the file so a failed fetch is
    // reported as a plain failure instead of metadata followed by one.
    let response = backend.storage.download(url).await.map_err(storage_error)?;

    let metadata = FileDownloadResponse {
        data: Some(file_download_response::Data::Metadata(file)),
    };
    if tx.send(Ok(metadata)).await.is_err() {
        return Ok(());
    }

    let mut body = response.bytes_stream();
    let mut buffer = BytesMut::new();
    while let Some(next) = body.next().await {
        let bytes = next.map_err(|err| {
            failure(
                CODE_UPSTREAM,
                "Upstream",
                format!("download stream failed: {err}"),
            )
        })?;
        buffer.extend_from_slice(&bytes);
        while buffer.len() >= DOWNLOAD_CHUNK_SIZE {
            let chunk = buffer.split_to(DOWNLOAD_CHUNK_SIZE);
            if !send_chunk(tx, chunk).await {
                return Ok(());
            }
        }
    }
    if !buffer.is_empty() {
        send_chunk(tx, buffer).await;
    }
    Ok(())
}

async fn send_chunk(
    tx: &tokio::sync::mpsc::Sender<Result<FileDownloadResponse, Status>>,
    chunk: BytesMut,
) -> bool {
    tx.send(Ok(FileDownloadResponse {
        data: Some(file_download_response::Data::Chunk(chunk.to_vec())),
    }))
    .await
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::{Multipart, State},
        http::{HeaderMap, StatusCode, header},
        routing::{get, post},
    };
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use serde::Serialize;
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::{Channel, Server};
    use tucana::{
        aquila::{
            FileUploadMetadata, action_file_service_client::ActionFileServiceClient,
            action_file_service_server::ActionFileServiceServer,
        },
        sagittarius_gateway::{
            GetFileDownloadUrlResponse,
            file_service_server::{FileService, FileServiceServer},
        },
        shared::{FileMetadata, ServiceError},
    };

    const BACKEND_TOKEN: &str = "backend-token";

    #[derive(Default, Debug)]
    struct Seen {
        authorization: Option<String>,
        fields: Vec<(String, String)>,
        file_name: Option<String>,
        content_type: Option<String>,
        bytes: Vec<u8>,
    }

    type SeenState = Arc<Mutex<Seen>>;

    /// Stand-in for the Sagittarius gateway's `POST /storage_objects/upload`.
    async fn mock_upload(
        State(seen): State<SeenState>,
        headers: HeaderMap,
        mut multipart: Multipart,
    ) -> (StatusCode, Json<serde_json::Value>) {
        let mut local = Seen {
            authorization: headers
                .get(header::AUTHORIZATION)
                .map(|v| v.to_str().unwrap().to_string()),
            ..Default::default()
        };
        while let Ok(Some(field)) = multipart.next_field().await {
            let name = field.name().unwrap_or_default().to_string();
            if name == "attachment" {
                local.file_name = field.file_name().map(String::from);
                local.content_type = field.content_type().map(String::from);
                local.bytes = field.bytes().await.unwrap().to_vec();
            } else {
                let value = field.text().await.unwrap();
                local.fields.push((name, value));
            }
        }
        let mut seen = seen.lock().unwrap();
        *seen = local;
        if seen.file_name.as_deref() == Some("reject.txt") {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"message": "quota exceeded", "error_code": "quota_exceeded"}),
                ),
            );
        }
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": 42,
                "file_name": seen.file_name,
                "content_type": seen.content_type,
                "byte_size": seen.bytes.len(),
                "checksum": "deadbeef",
            })),
        )
    }

    fn payload() -> Vec<u8> {
        (0..200_000u32).map(|i| (i % 251) as u8).collect()
    }

    struct MockFileService {
        base: String,
    }

    #[tonic::async_trait]
    impl FileService for MockFileService {
        async fn get_file_download_url(
            &self,
            request: Request<GetFileDownloadUrlRequest>,
        ) -> Result<Response<GetFileDownloadUrlResponse>, Status> {
            assert_eq!(
                request.metadata().get("authentication").unwrap(),
                BACKEND_TOKEN
            );
            let request = request.into_inner();
            if request.id == 404 {
                return Ok(Response::new(GetFileDownloadUrlResponse {
                    error: Some(ServiceError {
                        message: "file not found".into(),
                        details: vec![],
                    }),
                    ..Default::default()
                }));
            }
            assert_eq!(request.project_id, 7);
            assert_eq!(request.execution_identifier, "exec-1");
            let url = match request.id {
                // Relative, as Rails' signed paths are.
                42 => "/files/signed".to_string(),
                _ => format!("{}/files/gone", self.base),
            };
            Ok(Response::new(GetFileDownloadUrlResponse {
                url,
                file: Some(FileMetadata {
                    id: request.id,
                    file_name: "a.bin".into(),
                    content_type: "application/octet-stream".into(),
                    byte_size: payload().len() as i64,
                    checksum: "deadbeef".into(),
                }),
                error: None,
            }))
        }
    }

    #[derive(Serialize)]
    struct Claims {
        sub: String,
        exp: u64,
    }

    fn jwt(secret: &str, sub: &str) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            &Claims {
                sub: sub.into(),
                exp: 4_102_444_800,
            },
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    fn actions() -> ServiceConfiguration {
        let path =
            std::env::temp_dir().join(format!("aquila-file-test-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(
            &path,
            r#"{"actions":[{"token":"action-secret","identifier":"my-action"}]}"#,
        )
        .unwrap();
        let config = ServiceConfiguration::from_path(&path).unwrap();
        let _ = std::fs::remove_file(path);
        config
    }

    struct Harness {
        client: ActionFileServiceClient<Channel>,
        seen: SeenState,
        token: String,
    }

    async fn serve(addr_listener: TcpListener, router: Router) {
        tokio::spawn(async move {
            axum::serve(addr_listener, router).await.unwrap();
        });
    }

    async fn harness(max_file_size: u64, with_backend: bool) -> Harness {
        let seen = SeenState::default();

        // Sagittarius HTTP ingress mock.
        let http = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http_base = format!("http://{}", http.local_addr().unwrap());
        let body = payload();
        serve(
            http,
            Router::new()
                .route("/storage_objects/upload", post(mock_upload))
                .route("/files/signed", get(move || async move { body }))
                .route("/files/gone", get(|| async { StatusCode::NOT_FOUND }))
                .with_state(seen.clone()),
        )
        .await;

        // Sagittarius gRPC mock.
        let grpc = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let grpc_addr = grpc.local_addr().unwrap();
        let base = http_base.clone();
        tokio::spawn(async move {
            Server::builder()
                .add_service(FileServiceServer::new(MockFileService { base }))
                .serve_with_incoming(TcpListenerStream::new(grpc))
                .await
                .unwrap();
        });
        let channel = Channel::from_shared(format!("http://{grpc_addr}"))
            .unwrap()
            .connect_lazy();

        let backend = with_backend.then(|| FileBackend {
            files: SagittariusFileServiceClient::new(
                channel,
                BACKEND_TOKEN.into(),
                Duration::from_secs(5),
            ),
            storage: SagittariusStorageClient::new(
                format!("{http_base}/storage_objects/upload"),
                BACKEND_TOKEN.into(),
                Duration::from_secs(5),
            ),
        });

        // Aquila's ActionFileService under test.
        let aquila = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let aquila_addr = aquila.local_addr().unwrap();
        let service = AquilaActionFileService::new(actions(), backend, max_file_size);
        tokio::spawn(async move {
            Server::builder()
                .add_service(ActionFileServiceServer::new(service))
                .serve_with_incoming(TcpListenerStream::new(aquila))
                .await
                .unwrap();
        });
        let client = ActionFileServiceClient::connect(format!("http://{aquila_addr}"))
            .await
            .unwrap();

        Harness {
            client,
            seen,
            token: jwt("action-secret", "my-action"),
        }
    }

    fn metadata_message(file_name: &str) -> FileUploadRequest {
        FileUploadRequest {
            data: Some(file_upload_request::Data::Metadata(FileUploadMetadata {
                file_name: file_name.into(),
                content_type: "application/octet-stream".into(),
                expires_at: Some(1_900_000_000_000),
                project_id: 7,
                execution_identifier: "exec-1".into(),
            })),
        }
    }

    fn chunk_message(bytes: &[u8]) -> FileUploadRequest {
        FileUploadRequest {
            data: Some(file_upload_request::Data::Chunk(bytes.to_vec())),
        }
    }

    fn upload_request(
        token: Option<&str>,
        messages: Vec<FileUploadRequest>,
    ) -> Request<impl Stream<Item = FileUploadRequest> + use<>> {
        let mut request = Request::new(tokio_stream::iter(messages));
        if let Some(token) = token {
            request
                .metadata_mut()
                .insert("authorization", token.parse().unwrap());
        }
        request
    }

    fn download_request(token: Option<&str>, file: i64) -> Request<FileDownloadRequest> {
        let mut request = Request::new(FileDownloadRequest {
            id: file,
            project_id: 7,
            execution_identifier: "exec-1".into(),
        });
        if let Some(token) = token {
            request
                .metadata_mut()
                .insert("authorization", token.parse().unwrap());
        }
        request
    }

    fn upload_failure_code(response: FileUploadResponse) -> String {
        match response.data {
            Some(file_upload_response::Data::Failure(error)) => error.code,
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn upload_streams_chunks_to_sagittarius_as_multipart() {
        let mut h = harness(1024 * 1024, true).await;
        let body = payload();
        let mut messages = vec![metadata_message("a.bin")];
        messages.extend(body.chunks(50_000).map(chunk_message));

        let response = h
            .client
            .upload(upload_request(Some(&h.token), messages))
            .await
            .unwrap()
            .into_inner();

        let Some(file_upload_response::Data::File(file)) = response.data else {
            panic!("expected committed file, got {response:?}");
        };
        assert_eq!(file.id, 42);
        assert_eq!(file.byte_size, body.len() as i64);
        assert_eq!(file.checksum, "deadbeef");

        let seen = h.seen.lock().unwrap();
        assert_eq!(seen.authorization.as_deref(), Some("Bearer backend-token"));
        assert_eq!(seen.file_name.as_deref(), Some("a.bin"));
        assert_eq!(
            seen.content_type.as_deref(),
            Some("application/octet-stream")
        );
        assert_eq!(seen.bytes, body);
        for (name, value) in [
            ("project_id", "7"),
            ("execution_identifier", "exec-1"),
            ("expires_at", "1900000000000"),
        ] {
            assert!(
                seen.fields.contains(&(name.to_string(), value.to_string())),
                "missing field {name}: {:?}",
                seen.fields
            );
        }
    }

    #[tokio::test]
    async fn upload_maps_sagittarius_errors_to_failure() {
        let mut h = harness(1024, true).await;
        let response = h
            .client
            .upload(upload_request(
                Some(&h.token),
                vec![metadata_message("reject.txt"), chunk_message(b"hi")],
            ))
            .await
            .unwrap()
            .into_inner();

        let Some(file_upload_response::Data::Failure(error)) = response.data else {
            panic!("expected failure");
        };
        assert_eq!(error.code, "quota_exceeded");
        assert_eq!(error.message, "quota exceeded");
    }

    #[tokio::test]
    async fn upload_over_the_size_cap_fails() {
        let mut h = harness(100, true).await;
        let response = h
            .client
            .upload(upload_request(
                Some(&h.token),
                vec![
                    metadata_message("big.bin"),
                    chunk_message(&[0; 60]),
                    chunk_message(&[0; 60]),
                ],
            ))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(upload_failure_code(response), CODE_TOO_LARGE);
    }

    #[tokio::test]
    async fn upload_requires_metadata_first() {
        let mut h = harness(1024, true).await;
        let response = h
            .client
            .upload(upload_request(Some(&h.token), vec![chunk_message(b"data")]))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(upload_failure_code(response), CODE_INVALID_ARGUMENT);
    }

    #[tokio::test]
    async fn calls_without_a_valid_action_jwt_are_unauthenticated() {
        let mut h = harness(1024, true).await;

        let missing = h
            .client
            .upload(upload_request(None, vec![metadata_message("a")]))
            .await
            .unwrap_err();
        assert_eq!(missing.code(), tonic::Code::Unauthenticated);

        let wrong = h
            .client
            .upload(upload_request(
                Some(&jwt("other-secret", "my-action")),
                vec![metadata_message("a")],
            ))
            .await
            .unwrap_err();
        assert_eq!(wrong.code(), tonic::Code::Unauthenticated);

        let download = h
            .client
            .download(download_request(Some("not-a-jwt"), 42))
            .await
            .unwrap_err();
        assert_eq!(download.code(), tonic::Code::Unauthenticated);
    }

    async fn collect_download(h: &mut Harness, file: i64) -> Vec<file_download_response::Data> {
        let mut stream = h
            .client
            .download(download_request(Some(&h.token), file))
            .await
            .unwrap()
            .into_inner();
        let mut out = vec![];
        while let Some(message) = stream.message().await.unwrap() {
            out.push(message.data.unwrap());
        }
        out
    }

    #[tokio::test]
    async fn download_sends_metadata_then_bounded_chunks() {
        let mut h = harness(1024, true).await;
        let messages = collect_download(&mut h, 42).await;

        let mut iter = messages.into_iter();
        let Some(file_download_response::Data::Metadata(file)) = iter.next() else {
            panic!("first message must be metadata");
        };
        assert_eq!(file.file_name, "a.bin");

        let mut received = vec![];
        for message in iter {
            let file_download_response::Data::Chunk(chunk) = message else {
                panic!("expected only chunks after metadata");
            };
            assert!(chunk.len() <= DOWNLOAD_CHUNK_SIZE);
            received.extend(chunk);
        }
        assert_eq!(received, payload());
    }

    #[tokio::test]
    async fn download_errors_are_sent_as_failure() {
        let mut h = harness(1024, true).await;

        // Sagittarius reports the file as unavailable.
        let messages = collect_download(&mut h, 404).await;
        assert!(matches!(
            messages.as_slice(),
            [file_download_response::Data::Failure(e)] if e.message == "file not found"
        ));

        // The signed URL itself fails: no metadata is sent first.
        let messages = collect_download(&mut h, 7).await;
        assert!(matches!(
            messages.as_slice(),
            [file_download_response::Data::Failure(e)] if e.code == "HTTP_404"
        ));
    }

    #[tokio::test]
    async fn static_mode_reports_file_storage_unavailable() {
        let mut h = harness(1024, false).await;

        let response = h
            .client
            .upload(upload_request(Some(&h.token), vec![metadata_message("a")]))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(upload_failure_code(response), CODE_UNAVAILABLE);

        let messages = collect_download(&mut h, 42).await;
        assert!(matches!(
            messages.as_slice(),
            [file_download_response::Data::Failure(e)]
                if e.message == "file storage requires a Sagittarius connection"
        ));
    }
}
