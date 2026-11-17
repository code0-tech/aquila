//! Client for Sagittarius' `FileService`: resolves a storage object id to
//! a short-lived signed download URL (plus the file's metadata) so Aquila can
//! stream the bytes to an action without the action ever seeing the URL.

use crate::{authorization::authorization::get_authentication_metadata, telemetry::errors};
use std::time::Duration;
use tonic::{Extensions, Request, transport::Channel};
use tucana::sagittarius_gateway::{
    GetFileDownloadUrlRequest, GetFileDownloadUrlResponse, file_service_client::FileServiceClient,
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
        let mut request = Request::from_parts(
            get_authentication_metadata(&self.token),
            Extensions::new(),
            request,
        );
        request.set_timeout(self.unary_rpc_timeout);

        // Tonic clients are cheap to clone and need `&mut self` per call.
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
}
