//! HTTP side of file transfer toward Sagittarius. File bytes don't travel
//! over gRPC to Sagittarius: uploads are streamed as a multipart POST to the
//! Sagittarius gateway's HTTP ingress, and downloads are plain GETs of the
//! short-lived signed URL Sagittarius hands out (see
//! [`super::file_service_client_impl`]). Neither direction buffers a whole
//! file in memory.

use bytes::Bytes;
use futures::Stream;
use reqwest::{Body, Url, header::AUTHORIZATION, multipart};
use serde::Deserialize;
use std::time::Duration;
use tucana::shared::FileMetadata;

/// What the action declared about the file it is uploading.
#[derive(Clone, Debug)]
pub struct UploadParams {
    pub file_name: String,
    pub content_type: String,
    pub project_id: i64,
    pub execution_identifier: String,
    /// Epoch milliseconds.
    pub expires_at: Option<i64>,
}

/// Why an upload didn't produce a committed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageHttpError {
    /// Sagittarius answered with an error (`{ message, error_code }`).
    Rejected { code: String, message: String },
    /// Aquila couldn't talk to Sagittarius (connect error, bad response, aborted body).
    Transport(String),
}

#[derive(Deserialize)]
struct UploadSuccess {
    id: i64,
    file_name: String,
    content_type: String,
    byte_size: i64,
    checksum: String,
}

#[derive(Deserialize, Default)]
struct UploadFailure {
    #[serde(default)]
    message: String,
    #[serde(default)]
    error_code: Option<String>,
}

#[derive(Clone)]
pub struct SagittariusStorageClient {
    http: reqwest::Client,
    upload_url: String,
    token: String,
}

impl SagittariusStorageClient {
    /// `token` is the same Aquila backend token used for the gRPC clients.
    pub fn new(upload_url: String, token: String, connect_timeout: Duration) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(connect_timeout)
            .build()
            .expect("failed to build HTTP client for Sagittarius storage");
        Self {
            http,
            upload_url,
            token,
        }
    }

    /// Streams `body` to Sagittarius as the `attachment` part of a multipart
    /// POST. The body is forwarded chunk by chunk; if it yields an error the
    /// request is aborted.
    pub async fn upload<S>(
        &self,
        params: UploadParams,
        body: S,
    ) -> Result<FileMetadata, StorageHttpError>
    where
        S: Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync + 'static,
    {
        let part = multipart::Part::stream(Body::wrap_stream(body))
            .file_name(params.file_name)
            .mime_str(&params.content_type)
            .map_err(|err| StorageHttpError::Rejected {
                code: "INVALID_CONTENT_TYPE".into(),
                message: format!("invalid content type: {err}"),
            })?;

        let mut form = multipart::Form::new()
            .text("project_id", params.project_id.to_string())
            .text("execution_identifier", params.execution_identifier);
        if let Some(expires_at) = params.expires_at {
            form = form.text("expires_at", expires_at.to_string());
        }
        let form = form.part("attachment", part);

        let response = self
            .http
            .post(&self.upload_url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .multipart(form)
            .send()
            .await
            .map_err(|err| StorageHttpError::Transport(format!("upload request failed: {err}")))?;

        let status = response.status();
        let bytes = response.bytes().await.map_err(|err| {
            StorageHttpError::Transport(format!("failed to read upload response: {err}"))
        })?;

        if status.is_success() {
            let ok: UploadSuccess = serde_json::from_slice(&bytes).map_err(|err| {
                StorageHttpError::Transport(format!("malformed upload response: {err}"))
            })?;
            return Ok(FileMetadata {
                id: ok.id,
                file_name: ok.file_name,
                content_type: ok.content_type,
                byte_size: ok.byte_size,
                checksum: ok.checksum,
            });
        }

        let failure: UploadFailure = serde_json::from_slice(&bytes).unwrap_or_default();
        Err(StorageHttpError::Rejected {
            code: failure
                .error_code
                .unwrap_or_else(|| format!("HTTP_{}", status.as_u16())),
            message: if failure.message.is_empty() {
                format!("Sagittarius rejected the upload with HTTP {status}")
            } else {
                failure.message
            },
        })
    }

    /// Resolves a download URL from Sagittarius, which may be relative to
    /// the gateway (a path like `/storage_objects/download/...`), against
    /// the configured upload URL.
    pub fn resolve_download_url(&self, url: &str) -> Result<Url, String> {
        match Url::parse(url) {
            Ok(url) => Ok(url),
            Err(_) => Url::parse(&self.upload_url)
                .and_then(|base| base.join(url))
                .map_err(|err| format!("invalid download url: {err}")),
        }
    }

    /// Starts a GET of `url`; the caller streams the response body. No auth
    /// header is sent: the signed URL itself carries the authorization.
    pub async fn download(&self, url: Url) -> Result<reqwest::Response, StorageHttpError> {
        let response = self.http.get(url).send().await.map_err(|err| {
            StorageHttpError::Transport(format!("download request failed: {err}"))
        })?;

        if !response.status().is_success() {
            return Err(StorageHttpError::Rejected {
                code: format!("HTTP_{}", response.status().as_u16()),
                message: format!("file download failed with HTTP {}", response.status()),
            });
        }
        Ok(response)
    }
}
