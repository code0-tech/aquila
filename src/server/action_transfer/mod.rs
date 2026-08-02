//! gRPC server for the bidirectional `ActionTransfer` stream: authenticates
//! an action's logon, then bridges execution requests/results and config
//! updates between NATS and the connected action for the lifetime of the
//! stream.
//!
//! The module is split by responsibility:
//! - [`logon`] validates the initial logon message and registers the action.
//! - [`nats_bridge`] moves execution requests/results between NATS and gRPC.
//! - [`pending_replies`] tracks which NATS reply subject an execution result belongs to.

mod logon;
mod nats_bridge;
mod pending_replies;

use std::{pin::Pin, sync::Arc};

use futures::StreamExt;
use futures_core::Stream;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;
use tracing::Instrument;
use tucana::aquila::{
    ActionTransferRequest, ActionTransferResponse,
    action_transfer_service_server::ActionTransferService,
};

use crate::{
    configuration::service::ServiceConfiguration,
    sagittarius::module_service_client_impl::SagittariusModuleServiceClient, telemetry::metrics,
};

use logon::{extract_token, handle_logon};
use nats_bridge::{handle_event, handle_result, send_stream_error};
use pending_replies::PendingReplyStore;

/// Implements the `ActionTransfer` gRPC service that a connected action
/// speaks to for the lifetime of its bidirectional stream.
///
/// One instance is shared across all connected actions; per-connection state
/// (pending replies, logon status, ...) lives inside the `transfer` task
/// spawned for each stream instead of on `self`.
pub struct AquilaActionTransferServiceServer {
    client: async_nats::Client,
    kv: async_nats::jetstream::kv::Store,
    /// Static, pre-provisioned action tokens/configuration loaded at startup.
    actions: ServiceConfiguration,
    /// Present only in dynamic mode, where module updates must be relayed to Sagittarius.
    module_service: Option<Arc<Mutex<SagittariusModuleServiceClient>>>,
    /// Broadcasts module configuration updates to every connected action's config forwarder.
    action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
    /// Whether Aquila is running in static mode, which changes how config updates are sourced.
    is_static: bool,
}

impl AquilaActionTransferServiceServer {
    pub fn new(
        client: async_nats::Client,
        kv: async_nats::jetstream::kv::Store,
        actions: ServiceConfiguration,
        module_service: Option<Arc<Mutex<SagittariusModuleServiceClient>>>,
        action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
        is_static: bool,
    ) -> Self {
        Self {
            client,
            kv,
            actions,
            module_service,
            action_config_tx,
            is_static,
        }
    }
}

#[tonic::async_trait]
impl ActionTransferService for AquilaActionTransferServiceServer {
    type TransferStream =
        Pin<Box<dyn Stream<Item = Result<ActionTransferResponse, tonic::Status>> + Send + 'static>>;

    /// Opens the bidirectional stream an action uses to log on and then
    /// exchange events, execution requests, and results with Aquila.
    ///
    /// The gRPC protocol for this stream is a strict handshake: the first
    /// message must be a [`Logon`](tucana::aquila::action_transfer_request::Data::Logon),
    /// every message after that must not be, and everything is driven from a
    /// single spawned task so the stream can be read and written to concurrently.
    #[tracing::instrument(
        name = "aquila.action.transfer",
        skip_all,
        fields(rpc.system = "grpc", rpc.service = "ActionTransferService", rpc.method = "Transfer")
    )]
    async fn transfer(
        &self,
        request: tonic::Request<tonic::Streaming<ActionTransferRequest>>,
    ) -> std::result::Result<tonic::Response<Self::TransferStream>, tonic::Status> {
        let token = extract_token(&request)?;
        log::debug!("Action transfer stream opened");

        let mut first_request = true;
        let mut action_props: Option<tucana::aquila::ActionLogon> = None;
        let mut stream = request.into_inner();

        let actions = Arc::new(Mutex::new(self.actions.clone()));
        let kv = self.kv.clone();
        let client = self.client.clone();
        let module_service = self.module_service.clone();
        let cfg_tx = self.action_config_tx.clone();
        let is_static = self.is_static;
        let pending_replies = PendingReplyStore::new();

        let (tx, rx) =
            tokio::sync::mpsc::channel::<Result<ActionTransferResponse, tonic::Status>>(32);

        let stream_span = tracing::info_span!(
            "aquila.action.stream",
            action.identifier = tracing::field::Empty
        );
        tokio::spawn(async move {
            let mut cfg_forwarder_started = false;
            let mut connected_at = None;
            let mut connected_identifier = None;
            log::debug!("Action transfer stream started");

            while let Some(next) = stream.next().await {
                let transfer_request = match next {
                    Ok(tr) => tr,
                    Err(status) => {
                        log::warn!("Action transfer input stream failed status={:?}", status);
                        break;
                    }
                };

                let data = match transfer_request.data {
                    Some(d) => d,
                    None => {
                        log::warn!("Received empty action transfer request");
                        continue;
                    }
                };

                if first_request {
                    first_request = false;

                    match data {
                        tucana::aquila::action_transfer_request::Data::Logon(action_logon) => {
                            let identifier = match action_logon.module {
                                Some(ref m) => m.identifier.clone(),
                                None => {
                                    log::warn!("Rejected action logon reason=missing_module");
                                    send_stream_error(
                                        &tx,
                                        Status::aborted("Please provide a module configuration."),
                                    )
                                    .await;
                                    break;
                                }
                            };

                            log::debug!("Received logon for action {}", identifier);

                            let accepted = match handle_logon(
                                &token,
                                action_logon,
                                actions.clone(),
                                module_service.clone(),
                                client.clone(),
                                cfg_tx.clone(),
                                tx.clone(),
                                pending_replies.clone(),
                                &mut cfg_forwarder_started,
                            )
                            .await
                            {
                                Ok(v) => v,
                                Err(status) => {
                                    log::warn!(
                                        "Action logon failed identifier={} code={:?} message={}",
                                        identifier,
                                        status.code(),
                                        status.message()
                                    );
                                    send_stream_error(&tx, status).await;
                                    break;
                                }
                            };

                            action_props = Some(accepted);
                            tracing::Span::current()
                                .record("action.identifier", identifier.as_str());
                            metrics::action_connection(&identifier, "accepted");
                            metrics::action_active(&identifier, 1);
                            connected_at = Some(std::time::Instant::now());
                            connected_identifier = Some(identifier);
                        }
                        _ => {
                            log::error!("Action stream protocol violation expected=logon");
                            send_stream_error(
                                &tx,
                                Status::failed_precondition("first action stream message must be logon"),
                            )
                            .await;
                            break;
                        }
                    }

                    continue;
                }

                let props = match action_props.clone() {
                    Some(p) => p,
                    None => {
                        log::error!("Missing action properties after logon");
                        break;
                    }
                };

                let identifier = match props.module {
                    Some(ref m) => m.identifier.clone(),
                    None => {
                        log::error!("Logon state missing module");
                        break;
                    }
                };

                // Static mode has no Sagittarius to push config updates from, so
                // re-broadcast the action's configured values on every message it
                // sends instead of relying on a one-shot delivery at logon time.
                if is_static {
                    let lock = actions.lock().await;
                    let configs = lock.get_action_configuration(&token, &identifier);
                    for conf in configs {
                        if let Err(err) = cfg_tx.send(conf) {
                            log::warn!("No action configuration receivers available: {:?}", err);
                        }
                    }
                };

                match data {
                    tucana::aquila::action_transfer_request::Data::Logon(_) => {
                        log::warn!(
                            "Action stream protocol violation identifier={} reason=duplicate_logon",
                            identifier
                        );
                        send_stream_error(
                            &tx,
                            Status::failed_precondition("action stream logon was already accepted"),
                        )
                        .await;
                        break;
                    }
                    tucana::aquila::action_transfer_request::Data::Event(event) => {
                        log::debug!("Received event action={}", identifier);
                        metrics::action_event(&identifier);
                        handle_event(&identifier, event, kv.clone(), client.clone()).await;
                    }
                    tucana::aquila::action_transfer_request::Data::Result(execution_result) => {
                        log::debug!(
                            "Received execution result execution_id={} action={}",
                            execution_result.execution_identifier,
                            identifier
                        );

                        handle_result(
                            &identifier,
                            execution_result,
                            client.clone(),
                            pending_replies.clone(),
                        )
                        .await;
                    }
                }
            }

            if let Some(identifier) = connected_identifier {
                metrics::action_active(&identifier, -1);
                metrics::action_connection(&identifier, "closed");
                if let Some(connected_at) = connected_at {
                    metrics::action_connection_duration(
                        &identifier,
                        connected_at.elapsed().as_secs_f64(),
                    );
                }
            }
            log::debug!("Action transfer stream ended");
        }
        .instrument(stream_span));

        Ok(tonic::Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}
