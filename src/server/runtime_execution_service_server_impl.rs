//! gRPC server for `ExecutionService.Update`: the endpoint the Taurus
//! runtime posts execution results to, which are then relayed onto the
//! Sagittarius execution stream via [`SagittariusExecutionResponseSender`].

use crate::{
    authorization::authorization::extract_token, configuration::service::ServiceConfiguration,
    sagittarius::test_execution_client_impl::SagittariusExecutionResponseSender,
    server::action_transfer::ActionFlowExecutionRegistry,
};
use tonic::Status;
use tucana::aquila::execution_service_server::ExecutionService;
use tucana::aquila::{
    ActionFlowExecutionResponse, ActionTransferResponse, action_flow_execution_response,
    action_transfer_response,
};
use tucana::shared::{ExecutionResult, execution_result};

pub struct AquilaExecutionServiceServer {
    service_configuration: ServiceConfiguration,
    execution_response_sender: SagittariusExecutionResponseSender,
    /// Correlates execution ids Aquila dispatched on behalf of a connected
    /// action, so their result is routed back to that action's stream
    /// instead of Sagittarius' test execution stream.
    flow_execution_registry: ActionFlowExecutionRegistry,
}

impl AquilaExecutionServiceServer {
    pub fn new(
        service_configuration: ServiceConfiguration,
        execution_response_sender: SagittariusExecutionResponseSender,
        flow_execution_registry: ActionFlowExecutionRegistry,
    ) -> Self {
        Self {
            service_configuration,
            execution_response_sender,
            flow_execution_registry,
        }
    }
}

/// Converts a runtime's `ExecutionResult` into the `ActionFlowExecutionResponse`
/// sent back to the action that originally asked for this execution.
fn to_action_flow_execution_response(
    execution_result: ExecutionResult,
) -> ActionFlowExecutionResponse {
    let result = match execution_result.result {
        Some(execution_result::Result::Success(value)) => {
            Some(action_flow_execution_response::Result::Success(value))
        }
        Some(execution_result::Result::Error(error)) => {
            Some(action_flow_execution_response::Result::Failure(error))
        }
        None => None,
    };

    ActionFlowExecutionResponse {
        execution_identifier: execution_result.execution_identifier,
        result,
    }
}

fn execution_result_status(execution_result: &ExecutionResult) -> &'static str {
    match execution_result.result.as_ref() {
        Some(tucana::shared::execution_result::Result::Success(_)) => "success",
        Some(tucana::shared::execution_result::Result::Error(_)) => "error",
        None => "missing",
    }
}

#[tonic::async_trait]
impl ExecutionService for AquilaExecutionServiceServer {
    #[tracing::instrument(
        name = "aquila.execution.update",
        skip_all,
        fields(rpc.system = "grpc", rpc.service = "ExecutionService", rpc.method = "Update")
    )]
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::ExecutionRequest>,
    ) -> Result<tonic::Response<tucana::aquila::ExecutionResponse>, tonic::Status> {
        let token = match extract_token(&request) {
            Ok(t) => t.to_string(),
            Err(status) => {
                log::warn!("Rejected execution update reason=missing_or_invalid_token");
                return Err(status);
            }
        };

        // This endpoint is only ever called by the Taurus runtime, so the
        // token is checked against that fixed identifier rather than one
        // read from the request.
        if !self
            .service_configuration
            .has_runtime(&token, &String::from("taurus"))
        {
            log::warn!("Rejected execution update reason=token_not_registered runtime=taurus");
            return Err(Status::unauthenticated("token is not valid"));
        }
        log::debug!("Accepted execution update from runtime runtime=taurus");

        let execution_result = request.into_inner().execution_result.ok_or_else(|| {
            log::warn!("Rejected execution update reason=missing_execution_result");
            Status::invalid_argument("missing execution result")
        })?;

        let execution_id = execution_result.execution_identifier.clone();
        let flow_id = execution_result.flow_id;
        let result_status = execution_result_status(&execution_result);

        if let Some(action_tx) = self.flow_execution_registry.take(&execution_id).await {
            log::debug!(
                "Forwarding execution result to originating action stream execution_id={} flow_id={}",
                execution_id,
                flow_id
            );

            let resp = ActionTransferResponse {
                data: Some(action_transfer_response::Data::FlowExecutionResponse(
                    to_action_flow_execution_response(execution_result),
                )),
            };

            if action_tx.send(Ok(resp)).await.is_err() {
                log::warn!(
                    "Action stream closed before flow execution result could be delivered execution_id={}",
                    execution_id
                );
            }

            log::info!(
                "Delivered execution result to action stream execution_id={} flow_id={} result_status={}",
                execution_id,
                flow_id,
                result_status
            );

            return Ok(tonic::Response::new(tucana::aquila::ExecutionResponse {
                success: true,
            }));
        }

        log::debug!(
            "Forwarding execution result into Sagittarius stream execution_id={} flow_id={}",
            execution_id,
            flow_id
        );

        let forwarded_flow_id = self
            .execution_response_sender
            .send_execution_result(execution_result)
            .await?;

        log::info!(
            "Forwarded execution result into Sagittarius stream execution_id={} flow_id={} runtime_flow_id={} result_status={}",
            execution_id,
            forwarded_flow_id,
            flow_id,
            result_status
        );
        log::debug!(
            "Completed execution update execution_id={} flow_id={} runtime_flow_id={}",
            execution_id,
            forwarded_flow_id,
            flow_id
        );

        Ok(tonic::Response::new(tucana::aquila::ExecutionResponse {
            success: true,
        }))
    }
}
