//! Bridges NATS execution requests/results to and from a connected action's
//! gRPC stream: loads flows requested by id, forwards execution requests to
//! the action, and publishes results back to NATS.

use async_nats::{Subject, Subscriber};
use futures::StreamExt;
use prost::Message;
use tucana::{
    aquila::{
        ActionExecutionRequest, ActionExecutionResponse, ActionFlowExecutionRequest,
        ActionFlowExecutionResponse, ActionSubFlowExecutionRequest, ActionSubFlowExecutionResponse,
        ActionTransferResponse, action_flow_execution_response, action_sub_flow_execution_response,
        action_transfer_response,
    },
    shared::{Error, ExecutionFlow, ExecutionResult, execution_result},
};

use crate::{
    flow,
    telemetry::{errors, metrics},
    validation,
};

use super::flow_execution_registry::ActionFlowExecutionRegistry;
use super::pending_replies::{PendingReplyStore, pending_reply_keys};
use super::shard_registry::ShardAssignment;

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

    if !flow::flow_belongs_to_action(&validation_flow, action_identifier) {
        log::warn!(
            "Rejected action flow execution request for a flow it doesn't own action={} flow_id={}",
            action_identifier,
            flow_id
        );
        send_flow_execution_failure(&tx, execution_id, format!("flow {} was not found", flow_id))
            .await;
        return;
    }

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
        data: Some(action_transfer_response::Data::FlowExecutionResponse(
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
        )),
    };

    if tx.send(Ok(resp)).await.is_err() {
        log::debug!(
            "Action transfer response stream closed before flow execution failure could be sent"
        );
    }
}

/// A sub flow is the value of a function parameter that isn't a literal
/// (`shared.Value`) but the result of executing another flow. When the
/// action needs that value, it sends `ActionSubFlowExecutionRequest` back to
/// Aquila; this dispatches it to Taurus over a dedicated NATS request-reply
/// subject (distinct from the general `execution.<uuid>` bus, so Taurus can
/// tell the two request kinds apart) and relays the reply straight back to
/// the action as `ActionSubFlowExecutionResponse` - the same request/response
/// round trip [`forward_nats_to_action`]/[`handle_result`] drive for regular
/// node executions, just NATS request-reply instead of publish-then-reply-subject.
pub(super) async fn handle_sub_flow_execution(
    action_identifier: &str,
    request: ActionSubFlowExecutionRequest,
    nats_client: async_nats::Client,
    tx: tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
) {
    let execution_id = request.execution_identifier.clone();
    let correlation_id = request.correlation_identifier.clone();
    let topic = format!("sub_flow_execution.{}", execution_id);
    let bytes = request.encode_to_vec();

    log::debug!(
        "Requesting sub flow execution action={} execution_id={} topic={}",
        action_identifier,
        execution_id,
        topic
    );

    let reply = match nats_client.request(topic.clone(), bytes.into()).await {
        Ok(reply) => reply,
        Err(err) => {
            errors::record(
                "messaging",
                "action.sub_flow_execution.request",
                &err,
                format!(
                    "action.identifier={} execution_id={} topic={}",
                    action_identifier, execution_id, topic
                ),
            );
            send_sub_flow_execution_failure(
                &tx,
                execution_id,
                correlation_id,
                "failed to dispatch sub flow execution".to_string(),
            )
            .await;
            return;
        }
    };

    let execution_result = match ExecutionResult::decode(reply.payload) {
        Ok(result) => result,
        Err(err) => {
            errors::record(
                "protocol",
                "action.sub_flow_execution.decode",
                &err,
                format!(
                    "action.identifier={} execution_id={} topic={}",
                    action_identifier, execution_id, topic
                ),
            );
            send_sub_flow_execution_failure(
                &tx,
                execution_id,
                correlation_id,
                "failed to decode sub flow execution result".to_string(),
            )
            .await;
            return;
        }
    };

    let result = match execution_result.result {
        Some(execution_result::Result::Success(value)) => {
            Some(action_sub_flow_execution_response::Result::Success(value))
        }
        Some(execution_result::Result::Error(error)) => {
            Some(action_sub_flow_execution_response::Result::Failure(error))
        }
        None => None,
    };

    let resp = ActionTransferResponse {
        data: Some(action_transfer_response::Data::SubFlowExecutionResponse(
            ActionSubFlowExecutionResponse {
                execution_identifier: execution_id,
                correlation_identifier: correlation_id,
                result,
            },
        )),
    };

    if tx.send(Ok(resp)).await.is_err() {
        log::debug!(
            "Action transfer response stream closed before sub flow execution result could be sent"
        );
    }
}

/// Sends an immediate `ActionSubFlowExecutionResponse` failure without a
/// runtime having ever seen the request - used for dispatch/decode errors.
async fn send_sub_flow_execution_failure(
    tx: &tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
    execution_identifier: String,
    correlation_identifier: String,
    message: String,
) {
    let resp = ActionTransferResponse {
        data: Some(action_transfer_response::Data::SubFlowExecutionResponse(
            ActionSubFlowExecutionResponse {
                execution_identifier,
                correlation_identifier,
                result: Some(action_sub_flow_execution_response::Result::Failure(Error {
                    code: "A-SUB-FLOW-EXECUTION-000001".to_string(),
                    category: "Unavailable".to_string(),
                    message,
                    timestamp: validation::epoch_millis_now(),
                    version: crate::version::runtime_version().to_string(),
                    ..Default::default()
                })),
            },
        )),
    };

    if tx.send(Ok(resp)).await.is_err() {
        log::debug!(
            "Action transfer response stream closed before sub flow execution failure could be sent"
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
    shard: Option<ShardAssignment>,
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

        // The NATS subject is a plain fanout subscription shared by every
        // connection for this identifier, so every shard sees every request -
        // a `Split`-scaled connection only forwards the ones its shard owns
        // and silently leaves the rest for whichever connection does.
        if shard.is_some_and(|shard| !shard.owns(execution.project_id)) {
            log::debug!(
                "Execution request outside shard, dropping execution_id={} project_id={}",
                execution.execution_identifier,
                execution.project_id
            );
            continue;
        }

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
