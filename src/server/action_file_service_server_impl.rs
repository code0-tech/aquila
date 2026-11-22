//! gRPC `ActionFileService`: lets actions upload and download files without
//! file content ever passing through gRPC. Aquila only hands out short-lived
//! signed URLs obtained from the Sagittarius gateway; the action then
//! transfers the bytes over HTTP itself (PUT to upload, GET to download).
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

use std::time::{SystemTime, UNIX_EPOCH};

use tonic::{Request, Response, Status};
use tucana::{
    aquila::{
        FileDownloadUrl, FileDownloadUrlRequest, FileDownloadUrlResponse, FileUploadUrl,
        FileUploadUrlRequest, FileUploadUrlResponse, action_file_service_server::ActionFileService,
        file_download_url_response, file_upload_url_response,
    },
    sagittarius_gateway::{GetFileDownloadUrlRequest, GetFileUploadUrlRequest},
    shared::Error as SharedError,
};

use crate::{
    authorization::authorization::extract_token, configuration::service::ServiceConfiguration,
    sagittarius::file_service_client_impl::SagittariusFileServiceClient, telemetry::errors,
    version::runtime_version,
};

const CODE_UNAVAILABLE: &str = "AQ-FILE-0001";
const CODE_INVALID_ARGUMENT: &str = "AQ-FILE-0002";
const CODE_UPSTREAM: &str = "AQ-FILE-0004";

#[derive(Clone)]
pub struct AquilaActionFileService {
    actions: ServiceConfiguration,
    /// Client for the Sagittarius gateway; absent in static mode.
    files: Option<SagittariusFileServiceClient>,
}

impl AquilaActionFileService {
    pub fn new(actions: ServiceConfiguration, files: Option<SagittariusFileServiceClient>) -> Self {
        Self { actions, files }
    }

    /// Verifies the action's JWT and returns its identifier.
    fn authenticate<T>(&self, request: &Request<T>) -> Result<String, Status> {
        let token = extract_token(request)?;
        self.actions
            .authenticate_action(token)
            .map(str::to_string)
            .ok_or_else(|| {
                log::warn!("Rejected file request reason=invalid_action_token");
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

fn invalid_argument(message: &str) -> SharedError {
    failure(CODE_INVALID_ARGUMENT, "InvalidArgument", message)
}

fn transport_failure(status: &Status) -> SharedError {
    failure(
        CODE_UPSTREAM,
        "Upstream",
        format!("could not reach Sagittarius: {}", status.message()),
    )
}

fn upload_failure(error: SharedError) -> Response<FileUploadUrlResponse> {
    Response::new(FileUploadUrlResponse {
        data: Some(file_upload_url_response::Data::Failure(error)),
    })
}

fn download_failure(error: SharedError) -> Response<FileDownloadUrlResponse> {
    Response::new(FileDownloadUrlResponse {
        data: Some(file_download_url_response::Data::Failure(error)),
    })
}

fn validate_upload(request: &FileUploadUrlRequest) -> Result<(), &'static str> {
    if request.file_name.trim().is_empty() {
        return Err("file_name must not be empty");
    }
    if request.content_type.trim().is_empty() {
        return Err("content_type must not be empty");
    }
    if request.project_id <= 0 {
        return Err("project_id must be greater than zero");
    }
    Ok(())
}

fn validate_download(request: &FileDownloadUrlRequest) -> Result<(), &'static str> {
    if request.id <= 0 {
        return Err("id must be greater than zero");
    }
    if request.project_id <= 0 {
        return Err("project_id must be greater than zero");
    }
    Ok(())
}

#[tonic::async_trait]
impl ActionFileService for AquilaActionFileService {
    async fn request_upload(
        &self,
        request: Request<FileUploadUrlRequest>,
    ) -> Result<Response<FileUploadUrlResponse>, Status> {
        let action = self.authenticate(&request)?;
        let Some(files) = &self.files else {
            return Ok(upload_failure(unavailable()));
        };

        let request = request.into_inner();
        if let Err(message) = validate_upload(&request) {
            return Ok(upload_failure(invalid_argument(message)));
        }
        log::debug!(
            "File upload url requested action={} project_id={} execution={} name={:?}",
            action,
            request.project_id,
            request.execution_identifier,
            request.file_name
        );

        let response = match files
            .get_file_upload_url(GetFileUploadUrlRequest {
                file_name: request.file_name,
                content_type: request.content_type,
                expires_at: request.expires_at,
                project_id: request.project_id,
                execution_identifier: request.execution_identifier,
            })
            .await
        {
            Ok(response) => response,
            Err(status) => {
                errors::record_message(
                    "dependency",
                    "sagittarius.file.upload_url",
                    status.message(),
                    "",
                );
                return Ok(upload_failure(transport_failure(&status)));
            }
        };

        if let Some(error) = response.error {
            return Ok(upload_failure(failure(
                CODE_UPSTREAM,
                "Rejected",
                error.message,
            )));
        }
        if response.url.is_empty() {
            return Ok(upload_failure(failure(
                CODE_UPSTREAM,
                "Upstream",
                "Sagittarius returned no upload url",
            )));
        }

        Ok(Response::new(FileUploadUrlResponse {
            data: Some(file_upload_url_response::Data::Upload(FileUploadUrl {
                id: response.id,
                url: response.url,
                url_expires_at: response.url_expires_at,
            })),
        }))
    }

    async fn request_download(
        &self,
        request: Request<FileDownloadUrlRequest>,
    ) -> Result<Response<FileDownloadUrlResponse>, Status> {
        let action = self.authenticate(&request)?;
        let Some(files) = &self.files else {
            return Ok(download_failure(unavailable()));
        };

        let request = request.into_inner();
        if let Err(message) = validate_download(&request) {
            return Ok(download_failure(invalid_argument(message)));
        }
        log::debug!(
            "File download url requested action={} project_id={} execution={} id={}",
            action,
            request.project_id,
            request.execution_identifier,
            request.id
        );

        let response = match files
            .get_file_download_url(GetFileDownloadUrlRequest {
                id: request.id,
                project_id: request.project_id,
                execution_identifier: request.execution_identifier,
            })
            .await
        {
            Ok(response) => response,
            Err(status) => {
                errors::record_message(
                    "dependency",
                    "sagittarius.file.download_url",
                    status.message(),
                    "",
                );
                return Ok(download_failure(transport_failure(&status)));
            }
        };

        if let Some(error) = response.error {
            return Ok(download_failure(failure(
                CODE_UPSTREAM,
                "Rejected",
                error.message,
            )));
        }
        if response.url.is_empty() {
            return Ok(download_failure(failure(
                CODE_UPSTREAM,
                "Upstream",
                "Sagittarius returned no download url",
            )));
        }
        // Actions rely on the metadata (name, type, size) to handle the content.
        let Some(file) = response.file else {
            return Ok(download_failure(failure(
                CODE_UPSTREAM,
                "Upstream",
                "Sagittarius returned no file metadata",
            )));
        };

        Ok(Response::new(FileDownloadUrlResponse {
            data: Some(file_download_url_response::Data::Download(
                FileDownloadUrl {
                    url: response.url,
                    url_expires_at: response.url_expires_at,
                    file: Some(file),
                },
            )),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use serde::Serialize;
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::{Channel, Server};
    use tucana::{
        aquila::{
            action_file_service_client::ActionFileServiceClient,
            action_file_service_server::ActionFileServiceServer,
        },
        sagittarius_gateway::{
            GetFileDownloadUrlResponse, GetFileUploadUrlResponse,
            file_service_server::{FileService, FileServiceServer},
        },
        shared::{FileMetadata, ServiceError},
    };

    const BACKEND_TOKEN: &str = "backend-token";

    fn service_error(message: &str) -> Option<ServiceError> {
        Some(ServiceError {
            message: message.into(),
            details: vec![],
        })
    }

    /// Stand-in for the Sagittarius gateway's `FileService`.
    struct MockFileService;

    #[tonic::async_trait]
    impl FileService for MockFileService {
        async fn get_file_upload_url(
            &self,
            request: Request<GetFileUploadUrlRequest>,
        ) -> Result<Response<GetFileUploadUrlResponse>, Status> {
            assert_eq!(
                request.metadata().get("authentication").unwrap(),
                BACKEND_TOKEN
            );
            let request = request.into_inner();
            match request.file_name.as_str() {
                "reject.txt" => Ok(Response::new(GetFileUploadUrlResponse {
                    error: service_error("quota exceeded"),
                    ..Default::default()
                })),
                "boom.txt" => Err(Status::internal("gateway down")),
                _ => {
                    assert_eq!(request.content_type, "text/plain");
                    assert_eq!(request.expires_at, Some(1_900_000_000_000));
                    assert_eq!(request.project_id, 7);
                    assert_eq!(request.execution_identifier, "exec-1");
                    Ok(Response::new(GetFileUploadUrlResponse {
                        id: 42,
                        url: "https://storage.example/upload/signed".into(),
                        url_expires_at: 1_800_000_000_000,
                        error: None,
                    }))
                }
            }
        }

        async fn get_file_download_url(
            &self,
            request: Request<GetFileDownloadUrlRequest>,
        ) -> Result<Response<GetFileDownloadUrlResponse>, Status> {
            assert_eq!(
                request.metadata().get("authentication").unwrap(),
                BACKEND_TOKEN
            );
            let request = request.into_inner();
            match request.id {
                404 => Ok(Response::new(GetFileDownloadUrlResponse {
                    error: service_error("file not found"),
                    ..Default::default()
                })),
                500 => Err(Status::internal("gateway down")),
                204 => Ok(Response::new(GetFileDownloadUrlResponse {
                    url: "https://storage.example/download/signed".into(),
                    url_expires_at: 1_800_000_000_000,
                    file: None,
                    error: None,
                })),
                id => {
                    assert_eq!(request.project_id, 7);
                    assert_eq!(request.execution_identifier, "exec-1");
                    Ok(Response::new(GetFileDownloadUrlResponse {
                        url: "https://storage.example/download/signed".into(),
                        url_expires_at: 1_800_000_000_000,
                        file: Some(FileMetadata {
                            id,
                            file_name: "a.bin".into(),
                            content_type: "application/octet-stream".into(),
                            byte_size: 12,
                            checksum: "deadbeef".into(),
                            expires_at: None,
                        }),
                        error: None,
                    }))
                }
            }
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
        token: String,
    }

    async fn harness(with_backend: bool) -> Harness {
        // Sagittarius gRPC mock.
        let grpc = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let grpc_addr = grpc.local_addr().unwrap();
        tokio::spawn(async move {
            Server::builder()
                .add_service(FileServiceServer::new(MockFileService))
                .serve_with_incoming(TcpListenerStream::new(grpc))
                .await
                .unwrap();
        });
        let channel = Channel::from_shared(format!("http://{grpc_addr}"))
            .unwrap()
            .connect_lazy();

        let files = with_backend.then(|| {
            SagittariusFileServiceClient::new(channel, BACKEND_TOKEN.into(), Duration::from_secs(5))
        });

        // Aquila's ActionFileService under test.
        let aquila = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let aquila_addr = aquila.local_addr().unwrap();
        let service = AquilaActionFileService::new(actions(), files);
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
            token: jwt("action-secret", "my-action"),
        }
    }

    fn with_token<T>(token: Option<&str>, message: T) -> Request<T> {
        let mut request = Request::new(message);
        if let Some(token) = token {
            request
                .metadata_mut()
                .insert("authorization", token.parse().unwrap());
        }
        request
    }

    fn upload_request(token: Option<&str>, file_name: &str) -> Request<FileUploadUrlRequest> {
        with_token(
            token,
            FileUploadUrlRequest {
                file_name: file_name.into(),
                content_type: "text/plain".into(),
                expires_at: Some(1_900_000_000_000),
                project_id: 7,
                execution_identifier: "exec-1".into(),
            },
        )
    }

    fn download_request(token: Option<&str>, id: i64) -> Request<FileDownloadUrlRequest> {
        with_token(
            token,
            FileDownloadUrlRequest {
                id,
                project_id: 7,
                execution_identifier: "exec-1".into(),
            },
        )
    }

    fn upload_error(response: FileUploadUrlResponse) -> SharedError {
        match response.data {
            Some(file_upload_url_response::Data::Failure(error)) => error,
            other => panic!("expected failure, got {other:?}"),
        }
    }

    fn download_error(response: FileDownloadUrlResponse) -> SharedError {
        match response.data {
            Some(file_download_url_response::Data::Failure(error)) => error,
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn request_upload_returns_the_signed_url() {
        let mut h = harness(true).await;
        let response = h
            .client
            .request_upload(upload_request(Some(&h.token), "a.txt"))
            .await
            .unwrap()
            .into_inner();

        let Some(file_upload_url_response::Data::Upload(upload)) = response.data else {
            panic!("expected upload url, got {response:?}");
        };
        assert_eq!(upload.id, 42);
        assert_eq!(upload.url, "https://storage.example/upload/signed");
        assert_eq!(upload.url_expires_at, 1_800_000_000_000);
    }

    #[tokio::test]
    async fn request_download_returns_the_signed_url_and_metadata() {
        let mut h = harness(true).await;
        let response = h
            .client
            .request_download(download_request(Some(&h.token), 42))
            .await
            .unwrap()
            .into_inner();

        let Some(file_download_url_response::Data::Download(download)) = response.data else {
            panic!("expected download url, got {response:?}");
        };
        assert_eq!(download.url, "https://storage.example/download/signed");
        assert_eq!(download.url_expires_at, 1_800_000_000_000);
        let file = download.file.unwrap();
        assert_eq!(file.id, 42);
        assert_eq!(file.file_name, "a.bin");
    }

    #[tokio::test]
    async fn gateway_errors_are_mapped_to_failure() {
        let mut h = harness(true).await;

        let rejected = upload_error(
            h.client
                .request_upload(upload_request(Some(&h.token), "reject.txt"))
                .await
                .unwrap()
                .into_inner(),
        );
        assert_eq!(rejected.code, CODE_UPSTREAM);
        assert_eq!(rejected.message, "quota exceeded");

        let unreachable = upload_error(
            h.client
                .request_upload(upload_request(Some(&h.token), "boom.txt"))
                .await
                .unwrap()
                .into_inner(),
        );
        assert_eq!(unreachable.code, CODE_UPSTREAM);
        assert_eq!(unreachable.category, "Upstream");

        let missing = download_error(
            h.client
                .request_download(download_request(Some(&h.token), 404))
                .await
                .unwrap()
                .into_inner(),
        );
        assert_eq!(missing.code, CODE_UPSTREAM);
        assert_eq!(missing.message, "file not found");

        let unreachable = download_error(
            h.client
                .request_download(download_request(Some(&h.token), 500))
                .await
                .unwrap()
                .into_inner(),
        );
        assert_eq!(unreachable.code, CODE_UPSTREAM);
        assert_eq!(unreachable.category, "Upstream");

        let without_metadata = download_error(
            h.client
                .request_download(download_request(Some(&h.token), 204))
                .await
                .unwrap()
                .into_inner(),
        );
        assert_eq!(without_metadata.code, CODE_UPSTREAM);
        assert_eq!(
            without_metadata.message,
            "Sagittarius returned no file metadata"
        );
    }

    #[tokio::test]
    async fn invalid_arguments_are_rejected_before_reaching_the_gateway() {
        let mut h = harness(true).await;

        for (file_name, content_type, project_id) in [
            ("", "text/plain", 7),
            ("a.txt", "", 7),
            ("a.txt", "text/plain", 0),
        ] {
            let error = upload_error(
                h.client
                    .request_upload(with_token(
                        Some(&h.token),
                        FileUploadUrlRequest {
                            file_name: file_name.into(),
                            content_type: content_type.into(),
                            expires_at: None,
                            project_id,
                            execution_identifier: String::new(),
                        },
                    ))
                    .await
                    .unwrap()
                    .into_inner(),
            );
            assert_eq!(error.code, CODE_INVALID_ARGUMENT);
        }

        for (id, project_id) in [(0, 7), (42, 0)] {
            let error = download_error(
                h.client
                    .request_download(with_token(
                        Some(&h.token),
                        FileDownloadUrlRequest {
                            id,
                            project_id,
                            execution_identifier: String::new(),
                        },
                    ))
                    .await
                    .unwrap()
                    .into_inner(),
            );
            assert_eq!(error.code, CODE_INVALID_ARGUMENT);
        }
    }

    #[tokio::test]
    async fn calls_without_a_valid_action_jwt_are_unauthenticated() {
        let mut h = harness(true).await;

        let missing = h
            .client
            .request_upload(upload_request(None, "a.txt"))
            .await
            .unwrap_err();
        assert_eq!(missing.code(), tonic::Code::Unauthenticated);

        let wrong = h
            .client
            .request_upload(upload_request(
                Some(&jwt("other-secret", "my-action")),
                "a.txt",
            ))
            .await
            .unwrap_err();
        assert_eq!(wrong.code(), tonic::Code::Unauthenticated);

        let missing = h
            .client
            .request_download(download_request(None, 42))
            .await
            .unwrap_err();
        assert_eq!(missing.code(), tonic::Code::Unauthenticated);

        let wrong = h
            .client
            .request_download(download_request(Some("not-a-jwt"), 42))
            .await
            .unwrap_err();
        assert_eq!(wrong.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn static_mode_reports_file_storage_unavailable() {
        let mut h = harness(false).await;

        let upload = upload_error(
            h.client
                .request_upload(upload_request(Some(&h.token), "a.txt"))
                .await
                .unwrap()
                .into_inner(),
        );
        assert_eq!(upload.code, CODE_UNAVAILABLE);

        let download = download_error(
            h.client
                .request_download(download_request(Some(&h.token), 42))
                .await
                .unwrap()
                .into_inner(),
        );
        assert_eq!(download.code, CODE_UNAVAILABLE);
        assert_eq!(
            download.message,
            "file storage requires a Sagittarius connection"
        );
    }
}
