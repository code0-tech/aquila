/*
   Why is Aquila a client when Sagittarius wants a result of Aquila?

   In some conditions Sagittarius can't connect to Aquila
   Thus Aquila sends a `Logon` request to connect to Sagittarius establishing the connection
*/
use futures::StreamExt;
use prost::Message;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::{Extensions, Request, Status};
use tucana::sagittarius::execution_logon_request::Data;
use tucana::sagittarius::execution_service_client::ExecutionServiceClient;
use tucana::sagittarius::{ExecutionLogonRequest, Logon};
use tucana::shared::{ExecutionFlow, ExecutionResult, ValidationFlow};

use crate::authorization::authorization::get_authorization_metadata;

const EXECUTION_FLOW_ID_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_EXECUTION_FLOW_IDS: usize = 10_000;

struct ExecutionFlowIdMapping {
    flow_id: i64,
    expires_at: Instant,
}

#[derive(Clone, Default)]
pub struct SagittariusExecutionResponseSender {
    sender: Arc<Mutex<Option<tokio::sync::mpsc::Sender<ExecutionLogonRequest>>>>,
    execution_flow_ids: Arc<Mutex<HashMap<String, ExecutionFlowIdMapping>>>,
}

impl SagittariusExecutionResponseSender {
    pub fn new() -> Self {
        Self::default()
    }

    async fn attach(&self, sender: tokio::sync::mpsc::Sender<ExecutionLogonRequest>) {
        let mut current = self.sender.lock().await;
        let replacing_existing = current.is_some();
        *current = Some(sender);
        log::debug!(
            "Attached Sagittarius execution response sender replacing_existing={}",
            replacing_existing
        );
    }

    async fn clear(&self) {
        let mut current = self.sender.lock().await;
        let had_sender = current.is_some();
        *current = None;
        log::debug!(
            "Cleared Sagittarius execution response sender had_sender={}",
            had_sender
        );
    }

    async fn remember_execution_flow(&self, execution_id: &str, flow_id: i64) {
        if execution_id.is_empty() {
            log::warn!("Cannot remember execution flow_id because execution_id is empty");
            return;
        }

        let mut execution_flow_ids = self.execution_flow_ids.lock().await;
        let now = Instant::now();
        let expired_count = prune_expired_execution_flow_ids(&mut execution_flow_ids, now);
        let replacing_existing = execution_flow_ids.contains_key(execution_id);
        let evicted_execution_id =
            if !replacing_existing && execution_flow_ids.len() >= MAX_EXECUTION_FLOW_IDS {
                remove_oldest_execution_flow_id(&mut execution_flow_ids)
            } else {
                None
            };

        execution_flow_ids.insert(
            execution_id.to_string(),
            ExecutionFlowIdMapping {
                flow_id,
                expires_at: now + EXECUTION_FLOW_ID_TTL,
            },
        );

        if let Some(evicted_execution_id) = evicted_execution_id {
            log::warn!(
                "Evicted execution flow mapping because cache is full evicted_execution_id={} max_entries={}",
                evicted_execution_id,
                MAX_EXECUTION_FLOW_IDS
            );
        }
        log::debug!(
            "Remembered execution flow mapping execution_id={} flow_id={} cached_entries={} expired_entries={}",
            execution_id,
            flow_id,
            execution_flow_ids.len(),
            expired_count
        );
    }

    async fn forget_execution_flow(&self, execution_id: &str) {
        if execution_id.is_empty() {
            return;
        }

        let mut execution_flow_ids = self.execution_flow_ids.lock().await;
        let removed = execution_flow_ids.remove(execution_id).is_some();
        log::debug!(
            "Forgot execution flow mapping execution_id={} removed={}",
            execution_id,
            removed
        );
    }

    async fn take_execution_flow_id(&self, execution_id: &str) -> Option<i64> {
        if execution_id.is_empty() {
            return None;
        }

        let mut execution_flow_ids = self.execution_flow_ids.lock().await;
        let mapping = execution_flow_ids.remove(execution_id)?;
        if mapping.expires_at > Instant::now() {
            Some(mapping.flow_id)
        } else {
            log::debug!(
                "Dropped expired execution flow mapping execution_id={}",
                execution_id
            );
            None
        }
    }

    pub async fn send_execution_result(
        &self,
        mut execution_result: ExecutionResult,
    ) -> Result<i64, Status> {
        let execution_id = execution_result.execution_identifier.clone();

        if execution_result.flow_id == 0 {
            match self.take_execution_flow_id(&execution_id).await {
                Some(flow_id) if flow_id != 0 => {
                    log::warn!(
                        "Filled missing execution result flow_id from Aquila mapping execution_id={} flow_id={}",
                        execution_id,
                        flow_id
                    );
                    execution_result.flow_id = flow_id;
                }
                _ => {
                    log::warn!(
                        "Execution result has flow_id=0 and no Aquila mapping execution_id={}",
                        execution_id
                    );
                }
            }
        } else {
            self.forget_execution_flow(&execution_id).await;
        }

        let flow_id = execution_result.flow_id;
        let node_result_count = execution_result.node_execution_results.len();
        let result_status = execution_result_status(&execution_result);

        log::debug!(
            "Queueing execution result for Sagittarius stream execution_id={} flow_id={} result_status={} node_results={}",
            execution_id,
            flow_id,
            result_status,
            node_result_count
        );

        let sender = {
            let current = self.sender.lock().await;
            current.clone()
        };

        let Some(sender) = sender else {
            log::error!(
                "Cannot queue execution result for Sagittarius stream reason=stream_not_connected execution_id={} flow_id={} result_status={}",
                execution_id,
                flow_id,
                result_status
            );
            return Err(Status::unavailable(
                "sagittarius execution stream is not connected",
            ));
        };

        let remaining_capacity = sender.capacity();
        match sender
            .send(ExecutionLogonRequest {
                data: Some(Data::Response(execution_result)),
            })
            .await
        {
            Ok(()) => {
                log::debug!(
                    "Queued execution result for Sagittarius stream execution_id={} flow_id={} remaining_capacity_before_send={}",
                    execution_id,
                    flow_id,
                    remaining_capacity
                );
                Ok(flow_id)
            }
            Err(_) => {
                log::error!(
                    "Cannot queue execution result for Sagittarius stream reason=stream_closed execution_id={} flow_id={}",
                    execution_id,
                    flow_id
                );
                Err(Status::unavailable(
                    "sagittarius execution stream is closed",
                ))
            }
        }
    }
}

fn prune_expired_execution_flow_ids(
    execution_flow_ids: &mut HashMap<String, ExecutionFlowIdMapping>,
    now: Instant,
) -> usize {
    let initial_len = execution_flow_ids.len();
    execution_flow_ids.retain(|_, mapping| mapping.expires_at > now);
    initial_len - execution_flow_ids.len()
}

fn remove_oldest_execution_flow_id(
    execution_flow_ids: &mut HashMap<String, ExecutionFlowIdMapping>,
) -> Option<String> {
    let oldest_execution_id = execution_flow_ids
        .iter()
        .min_by_key(|(_, mapping)| mapping.expires_at)
        .map(|(execution_id, _)| execution_id.clone())?;

    execution_flow_ids.remove(&oldest_execution_id);
    Some(oldest_execution_id)
}

fn execution_result_status(execution_result: &ExecutionResult) -> &'static str {
    match execution_result.result.as_ref() {
        Some(tucana::shared::execution_result::Result::Success(_)) => "success",
        Some(tucana::shared::execution_result::Result::Error(_)) => "error",
        None => "missing",
    }
}

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

    async fn load_validation_flow(&self, flow_id: i64) -> Option<ValidationFlow> {
        match self.store.get(format!("{}.*", flow_id)).await {
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
                log::error!("Validation flow was not found flow_id={}", flow_id);
                None
            }
            Err(err) => {
                log::error!(
                    "Failed to fetch validation flow flow_id={} error={:?}",
                    flow_id,
                    err
                );
                None
            }
        }
    }

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
            get_authorization_metadata(&self.token),
            Extensions::new(),
            ack,
        );

        log::debug!("Opening Sagittarius execution stream");
        let mut test_execution_stream = match self.client.test(request).await {
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
