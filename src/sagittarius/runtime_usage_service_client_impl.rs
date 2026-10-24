//! Client for forwarding runtime usage -- the per-execution flow durations
//! Taurus group-commits into NATS -- to the Sagittarius gateway's
//! `RuntimeUsageService` (used by `sagittarius::runtime_usage_forwarder`).

use crate::{authorization::authorization::get_authentication_metadata, telemetry::errors};
use std::time::Duration;
use tonic::{Extensions, Request, transport::Channel};
use tucana::sagittarius_gateway::RuntimeUsageRequest;
use tucana::sagittarius_gateway::runtime_usage_service_client::RuntimeUsageServiceClient;
use tucana::shared::RuntimeUsage;

pub struct SagittariusRuntimeUsageServiceClient {
    client: RuntimeUsageServiceClient<Channel>,
    token: String,
    unary_rpc_timeout: Duration,
}

impl SagittariusRuntimeUsageServiceClient {
    pub fn new(channel: Channel, token: String, unary_rpc_timeout: Duration) -> Self {
        let client = RuntimeUsageServiceClient::new(channel);
        Self {
            client,
            token,
            unary_rpc_timeout,
        }
    }

    /// Sends one batch of usages and returns whether Sagittarius stored it,
    /// so the caller only acknowledges usages that were actually accepted.
    pub async fn update_runtime_usage(&mut self, runtime_usage: Vec<RuntimeUsage>) -> bool {
        let usage_count = runtime_usage.len();
        log::debug!(
            "Forwarding runtime usage to Sagittarius usage_count={}",
            usage_count
        );

        let mut request = Request::from_parts(
            get_authentication_metadata(&self.token),
            Extensions::new(),
            RuntimeUsageRequest { runtime_usage },
        );
        request.set_timeout(self.unary_rpc_timeout);

        let response = match self.client.update(request).await {
            Ok(response) => response.into_inner(),
            Err(err) => {
                errors::record(
                    "dependency",
                    "sagittarius.runtime_usage.update",
                    &err,
                    format!(
                        "code={} usage_count={} timeout_ms={}",
                        err.code(),
                        usage_count,
                        self.unary_rpc_timeout.as_millis()
                    ),
                );
                return false;
            }
        };

        if response.success {
            log::info!(
                "Sagittarius accepted runtime usage usage_count={}",
                usage_count
            );
        } else {
            log::warn!(
                "Sagittarius rejected runtime usage usage_count={} error={:?}",
                usage_count,
                response.error
            );
        }

        response.success
    }
}
