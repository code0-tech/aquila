//! Client for Sagittarius' `test` execution stream.
//!
//! Why is Aquila a client here when Sagittarius wants a result *from*
//! Aquila? In some deployments Sagittarius can't open a connection to
//! Aquila, so Aquila dials out and sends a `Logon` request first —
//! establishing the connection from Aquila's side even though Sagittarius
//! is the one issuing execution requests on it.
//!
//! - [`response_sender`] owns the outgoing half of the stream and is shared
//!   with every task that needs to report an execution result.
//! - [`flow_id_cache`] backs the piece of that sender that recovers a
//!   result's `flow_id` when a runtime doesn't echo it.

mod flow_id_cache;
mod response_sender;

pub use response_sender::SagittariusExecutionResponseSender;

use std::sync::Arc;

use futures::{StreamExt, TryStreamExt};
use prost::Message;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::{Extensions, Request};
use tucana::sagittarius_gateway::execution_logon_request::Data;
use tucana::sagittarius_gateway::execution_service_client::ExecutionServiceClient;
use tucana::sagittarius_gateway::{ExecutionLogonRequest, Logon};
use tucana::shared::{ExecutionFlow, ValidationFlow};

use crate::{authorization::authorization::get_authentication_metadata, flow::key_has_flow_id};

pub struct SagittariusTestExecutionServiceClient {
    nats_client: async_nats::Client,
    store: Arc<async_nats::jetstream::kv::Store>,
    client: ExecutionServiceClient<Channel>,
    token: String,
    response_sender: SagittariusExecutionResponseSender,
}

impl SagittariusTestExecutionServiceClient {
    pub fn new(
        nats_client: async_nats::Client,
        store: Arc<async_nats::jetstream::kv::Store>,
        channel: Channel,
        token: String,
        response_sender: SagittariusExecutionResponseSender,
    ) -> Self {
        let client = ExecutionServiceClient::new(channel);
        Self {
            nats_client,
            store,
            client,
            token,
            response_sender,
        }
    }

    /// Scans the flow KV bucket for the entry whose key encodes `flow_id`.
    /// There is no secondary index from flow id to key, so this is a linear
    /// scan of every stored flow.
    async fn load_validation_flow(&self, flow_id: i64) -> Option<ValidationFlow> {
        let mut keys = match self.store.keys().await {
            Ok(keys) => keys,
            Err(err) => {
                log::error!(
                    "Failed to list validation flow keys flow_id={} error={:?}",
                    flow_id,
                    err
                );
                return None;
            }
        };

        let key = loop {
            match keys.try_next().await {
                Ok(Some(key)) if key_has_flow_id(&key, flow_id) => break key,
                Ok(Some(_)) => {}
                Ok(None) => {
                    log::error!("Validation flow was not found flow_id={}", flow_id);
                    return None;
                }
                Err(err) => {
                    log::error!(
                        "Failed while scanning validation flow keys flow_id={} error={:?}",
                        flow_id,
                        err
                    );
                    return None;
                }
            }
        };

        log::debug!(
            "Resolved validation flow key flow_id={} key={}",
            flow_id,
            key
        );

        match self.store.get(&key).await {
            Ok(Some(bytes)) => match ValidationFlow::decode(bytes) {
                Ok(flow) => {
                    log::debug!(
                        "Loaded validation flow flow_id={} project_id={} starting_node_id={} node_functions={}",
                        flow.flow_id,
                        flow.project_id,
                        flow.starting_node_id,
                        flow.node_functions.len()
                    );
                    Some(flow)
                }
                Err(err) => {
                    log::error!(
                        "Failed to decode validation flow flow_id={} error={:?}",
                        flow_id,
                        err
                    );
                    None
                }
            },
            Ok(None) => {
                log::error!(
                    "Validation flow disappeared after key resolution flow_id={} key={}",
                    flow_id,
                    key
                );
                None
            }
            Err(err) => {
                log::error!(
                    "Failed to fetch validation flow flow_id={} key={} error={:?}",
                    flow_id,
                    key,
                    err
                );
                None
            }
        }
    }

    /// Opens the Sagittarius execution stream and services it until it ends.
    ///
    /// The stream's outgoing half is driven by an mpsc channel rather than a
    /// plain async generator so that [`SagittariusExecutionResponseSender`]
    /// can push execution results onto it from other tasks while this loop
    /// is busy reading incoming requests.
    pub async fn logon(&mut self) {
        let (tx, rx) = tokio::sync::mpsc::channel::<ExecutionLogonRequest>(10000);
        let logon = ExecutionLogonRequest {
            data: Some(Data::Logon(Logon {})),
        };

        self.response_sender.attach(tx.clone()).await;

        log::debug!("Queueing Sagittarius execution stream logon before opening stream");
        if let Err(err) = tx.send(logon).await {
            log::error!(
                "Failed to queue Sagittarius execution stream logon reason=channel_closed error={:?}",
                err
            );
            self.response_sender.clear().await;
            return;
        }
        log::info!("Sagittarius execution stream logon queued");

        let ack = ReceiverStream::new(rx);
        let request = Request::from_parts(
            get_authentication_metadata(&self.token),
            Extensions::new(),
            ack,
        );

        log::debug!("Opening Sagittarius execution stream");
        let mut test_execution_stream = match self.client.update(request).await {
            Ok(response) => {
                log::info!("Sagittarius execution stream established");
                response.into_inner()
            }
            Err(error) => {
                log::error!(
                    "Failed to establish Sagittarius execution stream code={} message={}",
                    error.code(),
                    error.message()
                );
                self.response_sender.clear().await;
                return;
            }
        };

        while let Some(next) = test_execution_stream.next().await {
            match next {
                Ok(test_execution_request) => {
                    if let Some(request) = test_execution_request.request {
                        log::info!(
                            "Received Sagittarius execution request requested_execution_id={} flow_id={} has_body={}",
                            request.execution_identifier,
                            request.flow_id,
                            request.body.is_some()
                        );
                        let validation_flow = match self.load_validation_flow(request.flow_id).await
                        {
                            Some(flow) => flow,
                            None => {
                                continue;
                            }
                        };

                        // TODO: When the new validator is ready, the body needs to be validated at this
                        // point.

                        let execution_id = if request.execution_identifier.is_empty() {
                            uuid::Uuid::new_v4().to_string()
                        } else {
                            request.execution_identifier.clone()
                        };
                        let generated_execution_id = request.execution_identifier.is_empty();

                        let execution_flow = ExecutionFlow {
                            flow_id: request.flow_id,
                            input_value: request.body,
                            starting_node_id: validation_flow.starting_node_id,
                            node_functions: validation_flow.node_functions,
                            project_id: validation_flow.project_id,
                        };

                        let bytes = execution_flow.encode_to_vec();
                        let payload_len = bytes.len();
                        let topic = format!("execution.{}", execution_id);

                        self.response_sender
                            .remember_execution_flow(&execution_id, execution_flow.flow_id)
                            .await;

                        log::debug!(
                            "Publishing execution request to NATS execution_id={} flow_id={} subject={} payload_bytes={} generated_execution_id={}",
                            execution_id,
                            execution_flow.flow_id,
                            topic,
                            payload_len,
                            generated_execution_id
                        );
                        match self.nats_client.publish(topic, bytes.into()).await {
                            Ok(_) => {
                                log::info!(
                                    "Published execution request to NATS execution_id={} flow_id={}",
                                    execution_id,
                                    execution_flow.flow_id
                                );
                            }
                            Err(err) => {
                                log::error!(
                                    "Failed to publish execution request execution_id={} flow_id={} error={:?}",
                                    execution_id,
                                    execution_flow.flow_id,
                                    err
                                );
                                self.response_sender
                                    .forget_execution_flow(&execution_id)
                                    .await;
                            }
                        }
                    } else {
                        log::warn!("Received empty Sagittarius execution stream message");
                    }
                }
                Err(status) => {
                    log::error!(
                        "Test execution stream error code={} message={}",
                        status.code(),
                        status.message()
                    );
                    break;
                }
            }
        }

        log::warn!("Sagittarius execution stream ended");
        self.response_sender.clear().await;
    }
}
