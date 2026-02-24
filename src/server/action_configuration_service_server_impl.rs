use crate::{configuration::action::ActionConfiguration, sagittarius::{action_configuration_service_client_impl::SagittariusActionConfigurationServiceClient, data_type_service_client_impl::SagittariusDataTypeServiceClient}};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Status;
use tucana::aquila::{action_configuration_service_server::ActionConfigurationService, data_type_service_server::DataTypeService};

pub struct AquilaActionConfigurationServiceServer {
    client: Arc<Mutex<SagittariusActionConfigurationServiceClient>>,
    actions: ActionConfiguration,
}

impl AquilaActionConfigurationServiceServer  {
    pub fn new(client: Arc<Mutex<SagittariusActionConfigurationServiceClient>>, actions: ActionConfiguration) -> Self {
        Self { client, actions }
    }
}

#[tonic::async_trait]
impl ActionConfigurationService for AquilaActionConfigurationServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::ActionConfigurationUpdateRequest>,
    ) -> Result<tonic::Response<tucana::aquila::ActionConfigurationUpdateResponse>, tonic::Status> {

        let token = match request.metadata().get("authorization") {
            Some(_) => todo!(),
            None => Err(Status::unauthenticated("")),
        };

        let data_type_update_request = request.into_inner();

        log::debug!(
            "Received DataTypes: {:?}",
            data_type_update_request
                .data_types
                .iter()
                .map(|d| d.identifier.clone())
                .collect::<Vec<_>>()
        );

        let mut client = self.client.lock().await;
        let response = client.update_data_types(data_type_update_request).await;

        Ok(tonic::Response::new(
            tucana::aquila::DataTypeUpdateResponse {
                success: response.success,
            },
        ))
    }
}
