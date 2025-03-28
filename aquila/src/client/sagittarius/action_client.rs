use aquila_grpc::get_authorization_metadata;
use async_trait::async_trait;
use log::{error, info};
use tonic::transport::Channel;
use tonic::{Extensions, Request, Response};
use tucana::aquila::InformationRequest;
use tucana::sagittarius::action_service_client::ActionServiceClient;
use tucana::sagittarius::{
    ActionLogoffRequest, ActionLogoffResponse, ActionLogonRequest, ActionLogonResponse,
};

/// Struct representing a service for sending flows received from an `Action` to `Sagittarius`.
/// Part that informs `Sagittarius`
pub struct SagittariusActionClientBase {
    client: ActionServiceClient<Channel>,
    token: String,
}

/// Trait representing a service for sending flows received from an `Action` to `Sagittarius`.
/// Part that informs `Sagittarius`
#[async_trait]
pub trait SagittariusActionClient {
    async fn new(sagittarius_url: String, token: String) -> SagittariusActionClientBase;
    async fn send_action_logon_request(
        &mut self,
        information: InformationRequest,
    ) -> Result<Response<ActionLogonResponse>, tonic::Status>;
    async fn send_action_logoff_request(
        &mut self,
        identifier: String,
    ) -> Result<Response<ActionLogoffResponse>, tonic::Status>;
}

/// Implementation of the service for sending flows received from an `Action` to `Sagittarius`.
/// Part that informs `Sagittarius`
#[async_trait]
impl SagittariusActionClient for SagittariusActionClientBase {
    /// Creates a connection to `Sagittarius`
    ///
    /// Behavior:
    /// Will panic when a connection can`t be established
    async fn new(sagittarius_url: String, token: String) -> SagittariusActionClientBase {
        let client = match ActionServiceClient::connect(sagittarius_url).await {
            Ok(res) => res,
            Err(start_error) => {
                panic!("Can't start client {:?}", start_error);
            }
        };

        SagittariusActionClientBase { client, token }
    }

    /// Sends `Sagittarius` the information that a `Action` went online.
    async fn send_action_logon_request(
        &mut self,
        information: InformationRequest,
    ) -> Result<Response<ActionLogonResponse>, tonic::Status> {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            ActionLogonRequest {
                identifier: information.identifier,
                function_definition: information.function_definition,
                parameter_definition: information.parameter_definition,
            },
        );

        match self.client.logon(request).await {
            Err(status) => {
                error!("Received a {status}, can't retrieve flows from Sagittarius");
                Err(status)
            }
            Ok(response) => {
                info!("Successfully reported action logon to sagittarius");
                Ok(response)
            }
        }
    }

    /// Sends `Sagittarius` the information that a `Action` went offline.
    async fn send_action_logoff_request(
        &mut self,
        identifier: String,
    ) -> Result<Response<ActionLogoffResponse>, tonic::Status> {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            ActionLogoffRequest { identifier },
        );

        match self.client.logoff(request).await {
            Err(status) => {
                error!("Received a {status}, can't retrieve flows from Sagittarius");
                Err(status)
            }
            Ok(response) => {
                info!("Successfully reported action logoff to sagittarius");
                Ok(response)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use tokio::task::JoinHandle;
    use tonic::{transport::Server, Request, Response, Status};
    use tucana::sagittarius::{
        action_service_server::{ActionService, ActionServiceServer},
        ActionLogoffRequest, ActionLogoffResponse, ActionLogonRequest, ActionLogonResponse,
    };
    use tucana::shared::{RuntimeFunctionDefinition, RuntimeParameterDefinition};

    #[derive(Debug, Default)]
    struct MockActionService;

    #[derive(Debug, Default)]
    struct BrokenMockActionService;

    #[tonic::async_trait]
    impl ActionService for MockActionService {
        async fn logon(
            &self,
            _request: Request<ActionLogonRequest>,
        ) -> Result<Response<ActionLogonResponse>, Status> {
            Ok(Response::new(ActionLogonResponse {})) // Mock response
        }

        async fn logoff(
            &self,
            _request: Request<ActionLogoffRequest>,
        ) -> Result<Response<ActionLogoffResponse>, Status> {
            Ok(Response::new(ActionLogoffResponse {}))
        }
    }

    #[tonic::async_trait]
    impl ActionService for BrokenMockActionService {
        async fn logon(
            &self,
            _request: Request<ActionLogonRequest>,
        ) -> Result<Response<ActionLogonResponse>, Status> {
            Err(Status::internal("This should simulate an error"))
        }

        async fn logoff(
            &self,
            _request: Request<ActionLogoffRequest>,
        ) -> Result<Response<ActionLogoffResponse>, Status> {
            Err(Status::internal("This should simulate an error"))
        }
    }

    async fn setup_sagittarius_mock() -> (JoinHandle<()>, String) {
        let addr_string = "127.0.0.1:50051";
        let addr: SocketAddr = addr_string.parse().unwrap();
        let mock_service = MockActionService::default();

        let server_handle = tokio::spawn(async move {
            Server::builder()
                .add_service(ActionServiceServer::new(mock_service))
                .serve(addr)
                .await
                .unwrap();
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        (server_handle, format!("http://{}", addr_string).to_string())
    }

    async fn setup_broken_sagittarius_mock() -> (JoinHandle<()>, String) {
        let addr_string = "127.0.0.1:50052";
        let addr: SocketAddr = addr_string.parse().unwrap();
        let mock_service = BrokenMockActionService::default();

        let server_handle = tokio::spawn(async move {
            Server::builder()
                .add_service(ActionServiceServer::new(mock_service))
                .serve(addr)
                .await
                .unwrap();
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        (server_handle, format!("http://{}", addr_string).to_string())
    }

    #[tokio::test]
    async fn test_sagittarius_action_client_integration() {
        let (sagittarius, url) = setup_sagittarius_mock().await;
        let mut client = SagittariusActionClientBase::new(url, String::from("")).await;

        let information = InformationRequest {
            identifier: "test_identifier".to_string(),
            function_definition: vec![RuntimeFunctionDefinition { id: "".to_string() }],
            parameter_definition: vec![RuntimeParameterDefinition {
                name: "".to_string(),
            }],
        };

        let logon_result = client.send_action_logon_request(information.clone()).await;
        assert!(logon_result.is_ok());

        let logoff_result = client
            .send_action_logoff_request(information.identifier.clone())
            .await;
        assert!(logoff_result.is_ok());
    }

    #[tokio::test]
    async fn test_broken_sagittarius_action_client_integration() {
        let (sagittarius, url) = setup_broken_sagittarius_mock().await;
        let mut client = SagittariusActionClientBase::new(url, String::from("")).await;

        let information: InformationRequest = InformationRequest {
            identifier: "test_identifier".to_string(),
            function_definition: vec![RuntimeFunctionDefinition { id: "".to_string() }],
            parameter_definition: vec![RuntimeParameterDefinition {
                name: "".to_string(),
            }],
        };

        let logon_result = client.send_action_logon_request(information.clone()).await;
        assert!(logon_result.is_err());

        let logoff_result = client
            .send_action_logoff_request(information.identifier.clone())
            .await;
        assert!(logoff_result.is_err());
        drop(sagittarius)
    }

    #[tokio::test]
    #[should_panic(expected = "Can't start client")]
    async fn test_sagittarius_action_client_new_should_panic() {
        let sagittarius_url = "http://127.0.0.1:12345".to_string();
        let _client = SagittariusActionClientBase::new(sagittarius_url, String::from("")).await;
    }
}
