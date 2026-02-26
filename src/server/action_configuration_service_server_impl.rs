use crate::{
    configuration::action::ActionConfiguration,
    sagittarius::action_configuration_service_client_impl::SagittariusActionConfigurationServiceClient,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Status;
use tucana::aquila::action_configuration_service_server::ActionConfigurationService;

pub struct AquilaActionConfigurationServiceServer {
    client: Arc<Mutex<SagittariusActionConfigurationServiceClient>>,
    actions: ActionConfiguration,
}

impl AquilaActionConfigurationServiceServer {
    pub fn new(
        client: Arc<Mutex<SagittariusActionConfigurationServiceClient>>,
        actions: ActionConfiguration,
    ) -> Self {
        Self { client, actions }
    }
}

#[tonic::async_trait]
impl ActionConfigurationService for AquilaActionConfigurationServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::ActionConfigurationUpdateRequest>,
    ) -> Result<tonic::Response<tucana::aquila::ActionConfigurationUpdateResponse>, tonic::Status>
    {
        let token = match request.metadata().get("authorization") {
            Some(ascii) => match ascii.to_str() {
                Ok(tk) => tk.to_string(),
                Err(err) => {
                    log::error!("Cannot read authorization header because: {:?}", err);
                    return Err(Status::internal("cannot read authorization header"));
                }
            },
            None => return Err(Status::unauthenticated("missing authorization token")),
        };

        let action_configuration_update_request = request.into_inner();
        match self.actions.clone().has_action(
            &token,
            &action_configuration_update_request.action_identifier,
        ) {
            true => {
                log::debug!(
                    "Action with identifer: {}, connected successfully",
                    action_configuration_update_request.action_identifier
                );
            }
            false => {
                log::debug!(
                    "Rejected action with identifer: {}, becuase its not registered",
                    action_configuration_update_request.action_identifier
                );
                return Err(Status::unauthenticated(""));
            }
        }

        let mut client = self.client.lock().await;
        let response = client
            .update_action_configuration(action_configuration_update_request)
            .await;

        Ok(tonic::Response::new(
            tucana::aquila::ActionConfigurationUpdateResponse {
                success: response.success,
            },
        ))
    }
}
