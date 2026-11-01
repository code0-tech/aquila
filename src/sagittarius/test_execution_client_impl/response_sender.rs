//! Queues execution results onto the outgoing half of the Sagittarius
//! execution logon stream, and fills in a result's `flow_id` from the
//! [`ExecutionFlowIdCache`] when a runtime didn't echo it back.

use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::Status;
use tucana::sagittarius_gateway::ExecutionLogonRequest;
use tucana::sagittarius_gateway::execution_logon_request::Data;
use tucana::shared::ExecutionResult;

use super::flow_id_cache::ExecutionFlowIdCache;
use super::result_trim::trim_execution_result_to_fit;

/// Handle shared between the task driving the Sagittarius execution stream
/// and every task that needs to report an execution result back to it.
///
/// The stream sender is `None` whenever no stream is currently connected
/// (before the first logon, or after a disconnect), in which case results
/// are rejected rather than buffered.
#[derive(Clone)]
pub struct SagittariusExecutionResponseSender {
    sender: Arc<Mutex<Option<tokio::sync::mpsc::Sender<ExecutionLogonRequest>>>>,
    execution_flow_ids: ExecutionFlowIdCache,
    /// Largest encoded `ExecutionResult` (bytes) sent onto the stream; larger
    /// results are trimmed to fit before sending. See
    /// [`crate::configuration::config::Grpc::max_execution_result_size`].
    max_execution_result_size: usize,
}

impl SagittariusExecutionResponseSender {
    pub fn new(max_execution_result_size: usize) -> Self {
        Self {
            sender: Arc::default(),
            execution_flow_ids: ExecutionFlowIdCache::default(),
            max_execution_result_size,
        }
    }

    pub(super) async fn attach(&self, sender: tokio::sync::mpsc::Sender<ExecutionLogonRequest>) {
        let mut current = self.sender.lock().await;
        let replacing_existing = current.is_some();
        *current = Some(sender);
        log::debug!(
            "Attached Sagittarius execution response sender replacing_existing={}",
            replacing_existing
        );
    }

    pub(super) async fn clear(&self) {
        let mut current = self.sender.lock().await;
        let had_sender = current.is_some();
        *current = None;
        log::debug!(
            "Cleared Sagittarius execution response sender had_sender={}",
            had_sender
        );
    }

    pub(super) async fn remember_execution_flow(&self, execution_id: &str, flow_id: i64) {
        self.execution_flow_ids
            .remember(execution_id, flow_id)
            .await;
    }

    pub(super) async fn forget_execution_flow(&self, execution_id: &str) {
        self.execution_flow_ids.forget(execution_id).await;
    }

    /// Sends `execution_result` to Sagittarius over the attached stream,
    /// recovering its `flow_id` from the cache first if the result didn't
    /// carry one.
    pub async fn send_execution_result(
        &self,
        mut execution_result: ExecutionResult,
    ) -> Result<i64, Status> {
        let execution_id = execution_result.execution_identifier.clone();

        if execution_result.flow_id == 0 {
            match self.execution_flow_ids.take(&execution_id).await {
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

        // Keep the result under the stream's gRPC message-size ceiling. An
        // oversized result would be rejected by Sagittarius' decoder and tear
        // down the whole stream, so we cut its largest values down to fit
        // instead - the execution is still reported, just without the
        // oversized payload.
        let cuts = trim_execution_result_to_fit(
            &mut execution_result,
            self.max_execution_result_size,
        );
        if cuts > 0 {
            log::warn!(
                "Trimmed oversized execution result to fit Sagittarius stream limit execution_id={} cuts={} limit={} final_size={}",
                execution_id,
                cuts,
                self.max_execution_result_size,
                prost::Message::encoded_len(&execution_result)
            );
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

fn execution_result_status(execution_result: &ExecutionResult) -> &'static str {
    match execution_result.result.as_ref() {
        Some(tucana::shared::execution_result::Result::Success(_)) => "success",
        Some(tucana::shared::execution_result::Result::Error(_)) => "error",
        None => "missing",
    }
}
