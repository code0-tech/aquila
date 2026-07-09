use crate::{authorization::authorization::get_authorization_metadata, telemetry::errors};
use std::time::Duration;
use tonic::transport::Channel;
use tonic::{Extensions, Request};

pub struct SagittariusModuleServiceClient {
    client: tucana::sagittarius::module_service_client::ModuleServiceClient<Channel>,
    token: String,
    unary_rpc_timeout: Duration,
}

impl SagittariusModuleServiceClient {
    pub fn new(channel: Channel, token: String, unary_rpc_timeout: Duration) -> Self {
        let client = tucana::sagittarius::module_service_client::ModuleServiceClient::new(channel);

        Self {
            client,
            token,
            unary_rpc_timeout,
        }
    }

    #[tracing::instrument(
        name = "sagittarius.module.update",
        skip_all,
        fields(rpc.system = "grpc", rpc.service = "ModuleService", rpc.method = "Update")
    )]
    pub async fn update_modules(
        &mut self,
        modules_update_request: tucana::aquila::ModuleUpdateRequest,
    ) -> tucana::aquila::ModuleUpdateResponse {
        let module_count = modules_update_request.modules.len();
        log::debug!(
            "Forwarding module update to Sagittarius module_count={}",
            module_count
        );

        let mut request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            tucana::sagittarius::ModuleUpdateRequest {
                modules: modules_update_request.modules,
            },
        );
        request.set_timeout(self.unary_rpc_timeout);

        match self.client.update(request).await {
            Ok(response) => {
                let res = response.into_inner();
                match res.success {
                    true => log::info!(
                        "Sagittarius successfully updated modules module_count={}",
                        module_count
                    ),
                    false => log::warn!(
                        "Sagittarius rejected module update module_count={} reason={:?}",
                        module_count,
                        res.error
                    ),
                };

                tucana::aquila::ModuleUpdateResponse {
                    success: res.success,
                }
            }
            Err(err) => {
                errors::record(
                    "dependency",
                    "sagittarius.module.update",
                    &err,
                    format!(
                        "module_count={} code={} timeout_ms={}",
                        module_count,
                        err.code(),
                        self.unary_rpc_timeout.as_millis()
                    ),
                );
                tucana::aquila::ModuleUpdateResponse { success: false }
            }
        }
    }
}
