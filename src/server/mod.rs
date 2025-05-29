use crate::sagittarius::{
    data_type_service_client_impl::SagittariusDataTypeServiceClient,
    flow_type_service_client_impl::SagittariusFlowTypeServiceClient,
    runtime_function_service_client_impl::SagittariusRuntimeFunctionServiceClient,
};
use data_type_service_server_impl::AquilaDataTypeServiceServer;
use flow_type_service_server_impl::AquilaFlowTypeServiceServer;
use log::info;
use runtime_function_service_server_impl::AquilaRuntimeFunctionServiceServer;
use std::net::SocketAddr;
use tonic::transport::Server;
use tucana::aquila::{
    data_type_service_server::DataTypeServiceServer,
    flow_type_service_server::FlowTypeServiceServer,
    runtime_function_definition_service_server::RuntimeFunctionDefinitionServiceServer,
};

mod data_type_service_server_impl;
mod flow_type_service_server_impl;
mod runtime_function_service_server_impl;

pub struct AquilaGRPCServer {
    token: String,
    sagittarius_url: String,
    address: SocketAddr,
}

impl AquilaGRPCServer {
    pub fn new(token: String, sagittarius_url: String, port: u16) -> Self {
        let address = match format!("[::1]:{}", port).parse() {
            Ok(addr) => {
                info!("Listening on {:?}", &addr);
                addr
            }
            Err(e) => panic!("Failed to parse address: {}", e),
        };

        AquilaGRPCServer {
            token,
            sagittarius_url,
            address,
        }
    }

    pub async fn start(&self) -> std::result::Result<(), tonic::transport::Error> {
        let data_type_service = SagittariusDataTypeServiceClient::new_arc(
            self.sagittarius_url.clone(),
            self.token.clone(),
        )
        .await;

        let flow_type_service = SagittariusFlowTypeServiceClient::new_arc(
            self.sagittarius_url.clone(),
            self.token.clone(),
        )
        .await;

        let runtime_function_service = SagittariusRuntimeFunctionServiceClient::new_arc(
            self.sagittarius_url.clone(),
            self.token.clone(),
        )
        .await;

        let data_type_server = AquilaDataTypeServiceServer::new(data_type_service.clone());
        let flow_type_server = AquilaFlowTypeServiceServer::new(flow_type_service.clone());
        let runtime_function_server =
            AquilaRuntimeFunctionServiceServer::new(runtime_function_service.clone());

        Server::builder()
            .add_service(DataTypeServiceServer::new(data_type_server))
            .add_service(FlowTypeServiceServer::new(flow_type_server))
            .add_service(RuntimeFunctionDefinitionServiceServer::new(
                runtime_function_server,
            ))
            .serve(self.address.clone())
            .await
    }
}
