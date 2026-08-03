//! Bridges NATS execution requests/results to and from a connected action's
//! gRPC stream: looks up matching flows for incoming events, forwards
//! execution requests to the action, and publishes results back to NATS.

use async_nats::{Subject, Subscriber};
use futures::StreamExt;
use prost::Message;
use tucana::{
    aquila::{
        ActionEvent, ActionExecutionRequest, ActionExecutionResponse, ActionFlowExecutionRequest,
        ActionFlowExecutionResponse, ActionTransferResponse, action_flow_execution_response,
    },
    shared::{Error, ExecutionFlow, Flows, ValidationFlow, Value},
};

use crate::{
    flow,
    telemetry::{errors, metrics},
    validation,
};

use super::flow_execution_registry::ActionFlowExecutionRegistry;
use super::pending_replies::{PendingReplyStore, pending_reply_keys};

/// Wraps the underlying NATS/KV error from a failed flow lookup so callers
/// get a stable, human-readable message while [`std::error::Error::source`]
/// still exposes the original cause for logging.
#[derive(Debug)]
pub(super) struct FlowIdentificationError {
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl std::fmt::Display for FlowIdentificationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("failed to identify flows")
    }
}

impl std::error::Error for FlowIdentificationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// Scans the flow KV bucket for every entry whose key matches `pattern`,
/// decoding each match. There is no secondary index for flows, so a full key
/// scan is the only lookup path available.
pub(super) async fn get_flows(
    pattern: String,
    kv: async_nats::jetstream::kv::Store,
) -> Result<Flows, FlowIdentificationError> {
    log::debug!("Scanning flows with pattern: {}", pattern);
    let mut collector = Vec::new();
    let mut keys = match kv.keys().await {
        Ok(keys) => keys.boxed(),
        Err(err) => {
            return Err(FlowIdentificationError {
                source: Box::new(err),
            });
        }
    };

    while let Ok(Some(key)) = tokio_stream::StreamExt::try_next(&mut keys).await {
        if !is_matching_key(&pattern, &key) {
            continue;
        }

        match kv.get(key.clone()).await {
            Ok(Some(bytes)) => {
                let decoded_flow = ValidationFlow::decode(bytes);
                match decoded_flow {
                    Ok(flow) => collector.push(flow),
                    Err(err) => {
                        errors::record(
                            "flow_storage",
                            "flow.decode",
                            &err,
                            format!("flow.key={key}"),
                        );
                    }
                }
            }
            Ok(None) => {
                log::debug!("Flow key disappeared while reading: {}", key);
            }
            Err(err) => {
                errors::record(
                    "flow_storage",
                    "flow.fetch",
                    &err,
                    format!("flow.key={key}"),
                );
            }
        }
    }

    log::debug!("Matched {} flows for pattern {}", collector.len(), pattern);
    Ok(Flows { flows: collector })
}

/// Matches a dot-separated KV key against a dot-separated pattern where `*`
/// matches any single segment. A pattern shorter than the key still counts
/// as a match on its own segments (the key's trailing segments are ignored).
fn is_matching_key(pattern: &String, key: &String) -> bool {
    let split_pattern = pattern.split(".");
    let split_key = key.split(".").collect::<Vec<&str>>();
    let zip = split_pattern.into_iter().zip(split_key);

    for (pattern_part, key_part) in zip {
        if pattern_part == "*" {
            continue;
        }

        if pattern_part != key_part {
            log::debug!("Key {} does not match pattern {}", key, pattern);
            return false;
        }
    }

    true
}

/// Turns a stored, pre-validated flow into the executable form sent to a
/// runtime, binding the triggering event's payload as its input value.
fn convert_validation_flow(flow: ValidationFlow, input_value: Option<Value>) -> ExecutionFlow {
    ExecutionFlow {
        flow_id: flow.flow_id,
        starting_node_id: flow.starting_node_id,
        input_value,
        node_functions: flow.node_functions,
        project_id: flow.project_id,
    }
}

/// Recovers the execution id from a NATS subject of the form
/// `action.<identifier>.<execution_id>`, used as a fallback when the
/// execution request's own payload doesn't carry one.
fn subject_execution_identifier(subject: &Subject) -> Option<String> {
    subject
        .as_str()
        .rsplit('.')
        .next()
        .filter(|execution_id| !execution_id.is_empty())
        .map(ToString::to_string)
}

/// Classifies an execution result for metrics purposes without cloning or
/// otherwise touching the underlying node result payload.
fn action_result_outcome(result: &ActionExecutionResponse) -> &'static str {
    match result
        .node_result
        .as_ref()
        .and_then(|node_result| node_result.result.as_ref())
    {
        Some(tucana::shared::node_execution_result::Result::Success(_)) => "success",
        Some(tucana::shared::node_execution_result::Result::Error(_)) => "error",
        None => "missing",
    }
}

/// Sends a terminal error down the gRPC response stream. Best-effort: if the
/// receiver already dropped the stream there's nothing left to notify.
pub(super) async fn send_stream_error(
    tx: &tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
    status: tonic::Status,
) {
    if tx.send(Err(status)).await.is_err() {
        log::debug!("Action transfer response stream closed before error could be sent");
    }
}

/// Looks up matching flows for an event and requests their execution.
///
/// Each match is dispatched as an independent NATS request-reply exchange on
/// its own `execution.<uuid>` subject, so one flow failing to find a runtime
/// doesn't block the others, and a runtime's response arrives correlated to
/// exactly the flow that triggered it.
pub(super) async fn handle_event(
    action_identifier: &str,
    event: ActionEvent,
    kv: async_nats::jetstream::kv::Store,
    client: async_nats::Client,
) {
    let pattern = format!("{}.*.{}.*", event.event_type, event.project_id);
    log::debug!(
        "Handling action event event_type={} project_id={}",
        event.event_type,
        event.project_id
    );

    let flows = match get_flows(pattern.clone(), kv).await {
        Ok(f) => f,
        Err(err) => {
            errors::record(
                "flow_storage",
                "action.event.find_flows",
                &err,
                format!(
                    "action.identifier={} event_type={} project_id={} pattern={}",
                    action_identifier, event.event_type, event.project_id, pattern
                ),
            );
            return;
        }
    };

    let matched_count = flows.flows.len();
    log::info!(
        "Matched flows for action event event_type={} project_id={} flow_count={}",
        event.event_type,
        event.project_id,
        matched_count
    );
    for flow in flows.flows {
        let uuid = uuid::Uuid::new_v4().to_string();
        let flow_id = flow.flow_id;
        let execution_flow: ExecutionFlow = convert_validation_flow(flow, event.payload.clone());
        let bytes = execution_flow.encode_to_vec();
        let topic = format!("execution.{}", uuid);

        log::info!(
            "Requesting execution flow_id={} execution_id={} event_type={} project_id={}",
            flow_id,
            uuid,
            event.event_type,
            event.project_id
        );

        if let Err(err) = client.request(topic.clone(), bytes.into()).await {
            errors::record(
                "messaging",
                "action.event.request_execution",
                &err,
                format!(
                    "action.identifier={} flow_id={} execution_id={} topic={}",
                    action_identifier, flow_id, uuid, topic
                ),
            );
        }
    }
}

/// Validates and dispatches a flow execution an action asked Aquila to run
/// onto the NATS execution bus, registering `tx` under the execution id so
/// the result (delivered separately once a runtime reports it, see
/// `runtime_execution_service_server_impl`) can be routed back to this
/// action's stream.
pub(super) async fn handle_flow_execution(
    action_identifier: &str,
    request: ActionFlowExecutionRequest,
    kv: async_nats::jetstream::kv::Store,
    nats_client: async_nats::Client,
    registry: ActionFlowExecutionRegistry,
    tx: tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
) {
    let execution_id = if request.execution_identifier.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        request.execution_identifier.clone()
    };

    let flow_id = match request.flow_id.parse::<i64>() {
        Ok(flow_id) => flow_id,
        Err(err) => {
            log::warn!(
                "Rejected action flow execution request due to invalid flow_id action={} flow_id={} error={}",
                action_identifier,
                request.flow_id,
                err
            );
            send_flow_execution_failure(&tx, execution_id, format!("invalid flow_id: {}", err))
                .await;
            return;
        }
    };

    let Some(validation_flow) = flow::load_validation_flow_by_id(&kv, flow_id).await else {
        send_flow_execution_failure(&tx, execution_id, format!("flow {} was not found", flow_id))
            .await;
        return;
    };

    if validation::is_rest_flow(&validation_flow) {
        let input_schema = validation::extract_input_schema(&validation_flow);
        if let Err(err) =
            validation::validate_body_against_schema(input_schema, request.payload.as_ref())
        {
            log::warn!(
                "Rejecting action flow execution request due to input schema validation failure action={} flow_id={} execution_id={} error={}",
                action_identifier,
                flow_id,
                execution_id,
                err
            );
            send_flow_execution_failure(&tx, execution_id, err.to_string()).await;
            return;
        }
    }

    let execution_flow = ExecutionFlow {
        flow_id,
        input_value: request.payload,
        starting_node_id: validation_flow.starting_node_id,
        node_functions: validation_flow.node_functions,
        project_id: validation_flow.project_id,
    };

    registry.insert(execution_id.clone(), tx.clone()).await;

    log::debug!(
        "Publishing action flow execution request to NATS action={} execution_id={} flow_id={}",
        action_identifier,
        execution_id,
        flow_id
    );

    if let Err(err) = flow::dispatch_execution(&nats_client, &execution_id, &execution_flow).await {
        errors::record(
            "messaging",
            "action.flow_execution.dispatch",
            &err,
            format!(
                "action.identifier={} execution_id={} flow_id={}",
                action_identifier, execution_id, flow_id
            ),
        );
        registry.take(&execution_id).await;
        send_flow_execution_failure(
            &tx,
            execution_id,
            "failed to dispatch execution".to_string(),
        )
        .await;
    }
}

/// Sends an immediate `ActionFlowExecutionResponse` failure without ever
/// dispatching to the execution bus - used for validation/lookup errors that
/// happen before dispatch.
async fn send_flow_execution_failure(
    tx: &tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
    execution_identifier: String,
    message: String,
) {
    let resp = ActionTransferResponse {
        data: Some(
            tucana::aquila::action_transfer_response::Data::FlowExecutionResponse(
                ActionFlowExecutionResponse {
                    execution_identifier,
                    result: Some(action_flow_execution_response::Result::Failure(Error {
                        code: "A-FLOW-EXECUTION-000001".to_string(),
                        category: "InvalidArgument".to_string(),
                        message,
                        timestamp: validation::epoch_millis_now(),
                        version: crate::version::runtime_version().to_string(),
                        ..Default::default()
                    })),
                },
            ),
        ),
    };

    if tx.send(Ok(resp)).await.is_err() {
        log::debug!(
            "Action transfer response stream closed before flow execution failure could be sent"
        );
    }
}

/// Publishes execution results back to the original NATS reply subject.
///
/// The reply subject was stashed by [`forward_nats_to_action`] when the
/// execution request first came in; the action only knows its own execution
/// id, not the NATS subject that's waiting for a reply, so this is the only
/// place that reunites the two.
pub(super) async fn handle_result(
    action_identifier: &str,
    execution_result: ActionExecutionResponse,
    client: async_nats::Client,
    pending_replies: PendingReplyStore,
) {
    let execution_id = execution_result.execution_identifier.clone();
    metrics::action_result(action_identifier, action_result_outcome(&execution_result));

    let Some(pending_reply) = pending_replies.remove(&execution_id).await else {
        metrics::action_failure(action_identifier, "result_unmatched");
        errors::record_message(
            "protocol",
            "action.result.match",
            "No pending NATS reply subject found",
            format!("action.identifier={action_identifier} execution_id={execution_id}"),
        );
        return;
    };
    metrics::action_execution_duration(
        action_identifier,
        pending_reply.started_at.elapsed().as_secs_f64(),
    );

    log::debug!(
        "Publishing execution result execution_id={} reply_subject={}",
        execution_id,
        pending_reply.reply_subject
    );

    let payload = execution_result.encode_to_vec();
    if let Err(err) = client
        .publish(pending_reply.reply_subject.clone(), payload.into())
        .await
    {
        metrics::action_failure(action_identifier, "result_publish");
        errors::record(
            "messaging",
            "action.result.publish",
            &err,
            format!(
                "action.identifier={} execution_id={} reply_subject={}",
                action_identifier, execution_id, pending_reply.reply_subject
            ),
        );
        return;
    }

    if let Err(err) = client.flush().await {
        metrics::action_failure(action_identifier, "result_flush");
        errors::record(
            "messaging",
            "action.result.flush",
            &err,
            format!("action.identifier={action_identifier} execution_id={execution_id}"),
        );
    }
}

/// Forwards NATS execution requests to the connected action via gRPC and stores reply subjects.
///
/// Runs for as long as the action's `action.<identifier>.*` subscription is
/// alive, which is the same lifetime as the action's logon; a fresh task is
/// spawned per logon rather than reused across reconnects.
pub(super) async fn forward_nats_to_action(
    action_identifier: String,
    mut sub: Subscriber,
    tx: tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
    pending_replies: PendingReplyStore,
) {
    log::debug!("Waiting for incoming action execution request");

    while let Some(msg) = sub.next().await {
        let mut execution = match ActionExecutionRequest::decode(msg.payload.as_ref()) {
            Ok(req) => req,
            Err(err) => {
                metrics::action_execution(&action_identifier, "invalid");
                metrics::action_failure(&action_identifier, "execution_decode");
                errors::record(
                    "protocol",
                    "action.execution.decode",
                    &err,
                    format!(
                        "action.identifier={} subject={} payload_bytes={}",
                        action_identifier,
                        msg.subject,
                        msg.payload.len()
                    ),
                );
                continue;
            }
        };

        let subject_execution_id = subject_execution_identifier(&msg.subject);
        if execution.execution_identifier.is_empty()
            && let Some(subject_execution_id) = subject_execution_id.as_ref()
        {
            log::warn!(
                "Filled missing action execution identifier from NATS subject subject={} execution_id={}",
                msg.subject,
                subject_execution_id
            );
            execution.execution_identifier = subject_execution_id.clone();
        }

        let execution_id = execution.execution_identifier.clone();

        let Some(reply_subject) = msg.reply.clone() else {
            metrics::action_execution(&action_identifier, "invalid");
            metrics::action_failure(&action_identifier, "missing_reply_subject");
            log::error!(
                "Received request without NATS reply subject execution_id={}",
                execution_id
            );
            continue;
        };

        let keys = pending_reply_keys(&execution_id, subject_execution_id.as_deref());
        if keys.is_empty() {
            metrics::action_execution(&action_identifier, "invalid");
            metrics::action_failure(&action_identifier, "missing_execution_identifier");
            log::error!(
                "Cannot store NATS reply subject without execution identifier subject={} reply_subject={}",
                msg.subject,
                reply_subject
            );
            continue;
        }

        pending_replies
            .insert(reply_subject.clone(), keys.clone())
            .await;

        log::debug!(
            "Stored reply subject reply_subject={} execution_id={} keys={:?}",
            reply_subject,
            execution_id,
            keys
        );

        log::debug!(
            "Forwarding execution request to action execution_id={} subject={}",
            execution_id,
            msg.subject
        );

        let resp = ActionTransferResponse {
            data: Some(tucana::aquila::action_transfer_response::Data::Execution(
                execution,
            )),
        };

        if tx.send(Ok(resp)).await.is_err() {
            metrics::action_execution(&action_identifier, "forward_failed");
            metrics::action_failure(&action_identifier, "execution_forward");
            log::debug!("Execution forwarder channel closed");

            // cleanup, since the request can no longer be delivered to the action
            pending_replies.remove(&execution_id).await;

            break;
        }
        metrics::action_execution(&action_identifier, "forwarded");
    }

    log::debug!("Execution forwarder stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_execution_identifier_uses_last_subject_token() {
        assert_eq!(
            subject_execution_identifier(&Subject::from("action.example.execution-id")),
            Some("execution-id".to_string())
        );
    }

    #[test]
    fn action_result_outcome_distinguishes_success_error_and_missing() {
        use tucana::shared::{Error, NodeExecutionResult, Value, node_execution_result};

        let response = |result| ActionExecutionResponse {
            execution_identifier: "execution-id".into(),
            node_result: Some(NodeExecutionResult {
                result,
                ..Default::default()
            }),
        };

        assert_eq!(
            action_result_outcome(&response(Some(node_execution_result::Result::Success(
                Value::default()
            )))),
            "success"
        );
        assert_eq!(
            action_result_outcome(&response(Some(node_execution_result::Result::Error(
                Error::default()
            )))),
            "error"
        );
        assert_eq!(action_result_outcome(&response(None)), "missing");
        assert_eq!(
            action_result_outcome(&ActionExecutionResponse::default()),
            "missing"
        );
    }
}
