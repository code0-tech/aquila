/*
   Why is Aquila a client when Sagittarius wants a result of Aquila?

   In some conditions Sagittarius can't connect to Aquila
   Thus Aquila sends a `Logon` request to connect to Sagittarius establishing the connection
*/
use futures::StreamExt;
use prost::Message;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::{Request};
use tucana::sagittarius::execution_logon_request::Data;
use tucana::sagittarius::execution_service_client::ExecutionServiceClient;
use tucana::sagittarius::{ExecutionLogonRequest, Logon, TestExecutionResponse};
use tucana::shared::{ExecutionFlow, ValidationFlow, Value};

pub struct SagittariusTestExecutionServiceClient {
    nats_client: async_nats::Client,
    store: Arc<async_nats::jetstream::kv::Store>,
    client: ExecutionServiceClient<Channel>,
    token: String,
}

impl SagittariusTestExecutionServiceClient {
    pub async fn new(
        nats_client: async_nats::Client,
        store: Arc<async_nats::jetstream::kv::Store>,
        sagittarius_url: String,
        token: String,
    ) -> Self {
        let client = match ExecutionServiceClient::connect(sagittarius_url).await {
            Ok(client) => {
                log::info!("Successfully connected to Sagittarius RuntimeFunction Endpoint!");
                client
            }
            Err(err) => panic!(
                "Failed to connect to Sagittarius (RuntimeFunction Endpoint): {:?}",
                err
            ),
        };

        Self {
            nats_client,
            store,
            client,
            token,
        }
    }

    async fn load_validation_flow(
        &self,
        flow_id: i64,
    ) -> Option<ValidationFlow> {
        match self.store.get(format!("{}.*", flow_id)).await {
            Ok(Some(bytes)) => match ValidationFlow::decode(bytes) {
                Ok(flow) => Some(flow),
                Err(err) => {
                    log::error!("Cannot decode ValidationFlow for {}: {:?}", flow_id, err);
                    None
                }
            },
            Ok(None) => {
                log::error!("No flow found with id: {}", flow_id);
                None
            }
            Err(err) => {
                log::error!("Error fetching flow {}: {:?}", flow_id, err);
                None
            }
        }
    }

    pub async fn logon(&mut self) {
        let (tx, rx) = tokio::sync::mpsc::channel::<ExecutionLogonRequest>(10000);
        let ack = ReceiverStream::new(rx);
        let logon = ExecutionLogonRequest { data: Some(Data::Logon(Logon {})), };

        let mut test_execution_stream =  match self.client.test(Request::new(ack)).await {
            Ok(response) => response.into_inner(),
            Err(error) => {
                log::error!("Received status code: {:?}", error);
                return;
            },
        };

        tx.send(logon).await.unwrap();

        while let Some(Ok(test_execution_request)) = test_execution_stream.next().await {
            match test_execution_request.request {
                Some(request) => {
                    let validation_flow = match self.load_validation_flow(request.flow_id).await {
                        Some(flow) => flow,
                        None => {
                            return;
                        }
                    };

                    let uuid = uuid::Uuid::new_v4().to_string();
                    let execution_flow = ExecutionFlow {
                        flow_id: request.flow_id,
                        starting_node: validation_flow.starting_node,
                        input_value: request.body,
                    };

                    let bytes = execution_flow.encode_to_vec();
                    let topic = format!("test_execution.{}", uuid);
                    let result = self.nats_client.request(topic, bytes.into()).await;

                    match result {
                        Ok(message) => match Value::decode(message.payload) {
                            Ok(value) => {

                                let execution_result = ExecutionLogonRequest {
                                    data: Some(Data::Response(TestExecutionResponse {
                                        flow_id: request.flow_id,
                                        execution_uuid: uuid,
                                        result: Some(value),
                                        logs: vec![],
                                    })),
                                };

                                if let Err(err) = tx.send(execution_result).await {
                                    log::error!("Failed to send ExecutionLogonResponse: {:?}", err);
                                }
                            }
                            Err(err) => {
                                log::error!("Failed to decode response from NATS server: {:?}", err);
                            }
                        },
                        Err(err) => {
                            log::error!("Failed to send request to NATS server: {:?}", err);

                        }
                    }
                }
                _ => {}
            }
        }
    }
}
