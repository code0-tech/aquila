//! Client for Sagittarius' `FileService`: obtains short-lived signed upload
//! and download URLs that Aquila hands to actions, which transfer the bytes
//! over HTTP themselves.

use crate::{authorization::authorization::get_authentication_metadata, telemetry::errors};
use std::time::Duration;
use tonic::{Extensions, Request, transport::Channel};
use tucana::sagittarius_gateway::{
    GetFileDownloadUrlRequest, GetFileDownloadUrlResponse, GetFileUploadUrlRequest,
    GetFileUploadUrlResponse, file_service_client::FileServiceClient,
};

#[derive(Clone)]
pub struct SagittariusFileServiceClient {
    client: FileServiceClient<Channel>,
    token: String,
    unary_rpc_timeout: Duration,
}

impl SagittariusFileServiceClient {
    pub fn new(channel: Channel, token: String, unary_rpc_timeout: Duration) -> Self {
        Self {
            client: FileServiceClient::new(channel),
            token,
            unary_rpc_timeout,
        }
    }

    #[tracing::instrument(
        name = "sagittarius.file.get_upload_url",
        skip_all,
        fields(rpc.system = "grpc", rpc.service = "FileService", rpc.method = "GetFileUploadUrl")
    )]
    /// Asks Sagittarius for an upload URL. Sagittarius authorizes the
    /// supplied project/execution context.
    pub async fn get_file_upload_url(
        &self,
        request: GetFileUploadUrlRequest,
    ) -> Result<GetFileUploadUrlResponse, tonic::Status> {
        let request = self.request(request);

        // Tonic clients are cheap to clone and need `&mut self` per call.
        let mut client = self.client.clone();
        match client.get_file_upload_url(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(err) => {
                errors::record(
                    "dependency",
                    "sagittarius.file.get_upload_url",
                    &err,
                    format!("code={}", err.code()),
                );
                Err(err)
            }
        }
    }

    #[tracing::instrument(
        name = "sagittarius.file.get_download_url",
        skip_all,
        fields(rpc.system = "grpc", rpc.service = "FileService", rpc.method = "GetFileDownloadUrl")
    )]
    /// Asks Sagittarius for a download URL for `request.id`. Sagittarius
    /// authorizes the id against the supplied project/execution context.
    pub async fn get_file_download_url(
        &self,
        request: GetFileDownloadUrlRequest,
    ) -> Result<GetFileDownloadUrlResponse, tonic::Status> {
        let request = self.request(request);

        let mut client = self.client.clone();
        match client.get_file_download_url(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(err) => {
                errors::record(
                    "dependency",
                    "sagittarius.file.get_download_url",
                    &err,
                    format!("code={}", err.code()),
                );
                Err(err)
            }
        }
    }

    fn request<T>(&self, message: T) -> Request<T> {
        let mut request = Request::from_parts(
            get_authentication_metadata(&self.token),
            Extensions::new(),
            message,
        );
        request.set_timeout(self.unary_rpc_timeout);
        request
    }
}
