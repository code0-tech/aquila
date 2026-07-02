use crate::{
    authorization::authorization::extract_token, configuration::service::ServiceConfiguration,
    sagittarius::test_execution_client_impl::SagittariusExecutionResponseSender,
};
use tonic::Status;
use tucana::aquila::execution_service_server::ExecutionService;
use tucana::shared::ExecutionResult;

pub struct AquilaExecutionServiceServer {
    service_configuration: ServiceConfiguration,
    execution_response_sender: SagittariusExecutionResponseSender,
}

impl AquilaExecutionServiceServer {
    pub fn new(
        service_configuration: ServiceConfiguration,
        execution_response_sender: SagittariusExecutionResponseSender,
    ) -> Self {
        Self {
            service_configuration,
            execution_response_sender,
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

#[tonic::async_trait]
impl ExecutionService for AquilaExecutionServiceServer {
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
