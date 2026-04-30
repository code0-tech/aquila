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
use tonic::{Extensions, Request};
use tucana::sagittarius::execution_logon_request::Data;
use tucana::sagittarius::execution_service_client::ExecutionServiceClient;
use tucana::sagittarius::{ExecutionLogonRequest, Logon};
use tucana::shared::{ExecutionFlow, ExecutionResult, ValidationFlow};

use crate::authorization::authorization::get_authorization_metadata;

pub struct SagittariusTestExecutionServiceClient {
    nats_client: async_nats::Client,
    store: Arc<async_nats::jetstream::kv::Store>,
    client: ExecutionServiceClient<Channel>,
    token: String,
}

impl SagittariusTestExecutionServiceClient {
    pub fn new(
        nats_client: async_nats::Client,
        store: Arc<async_nats::jetstream::kv::Store>,
        channel: Channel,
        token: String,
    ) -> Self {
        let client = ExecutionServiceClient::new(channel);
        Self {
            nats_client,
            store,
            client,
            token,
        }
    }

    async fn load_validation_flow(&self, flow_id: i64) -> Option<ValidationFlow> {
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
        let logon = ExecutionLogonRequest {
            data: Some(Data::Logon(Logon {})),
        };
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            ack,
        );

        let mut test_execution_stream = match self.client.test(request).await {
            Ok(response) => response.into_inner(),
            Err(error) => {
                log::error!("Received status code: {:?}", error);
                return;
            }
        };

        if let Err(err) = tx.send(logon).await {
            log::error!("Failed to send test execution logon: {:?}", err);
            match test_execution_stream.message().await {
                Ok(Some(_)) => {
                    log::warn!("Test execution stream produced data even though logon send failed");
                }
                Ok(None) => {
                    log::warn!("Test execution stream closed before logon could be sent");
                }
                Err(status) => {
                    log::error!(
                        "Test execution stream closed with status code={} message={}",
                        status.code(),
                        status.message()
                    );
                }
            }
            return;
        }

        while let Some(next) = test_execution_stream.next().await {
            match next {
                Ok(test_execution_request) => {
                    if let Some(request) = test_execution_request.request {
                        let validation_flow = match self.load_validation_flow(request.flow_id).await
                        {
                            Some(flow) => flow,
                            None => {
                                continue;
                            }
                        };

                        let uuid = uuid::Uuid::new_v4().to_string();

                        // TODO: When the new validator is ready, the body needs to be validated at this
                        // point.

                        let execution_flow = ExecutionFlow {
                            flow_id: request.flow_id,
                            input_value: request.body,
                            starting_node_id: validation_flow.starting_node_id,
                            node_functions: validation_flow.node_functions,
                            project_id: validation_flow.project_id,
                        };

                        let bytes = execution_flow.encode_to_vec();
                        let topic = format!("test_execution.{}", uuid);
                        let result = self.nats_client.request(topic, bytes.into()).await;

                        // Aquila will expect a `Execution Result` back from Taurus
                        match result {
                            Ok(message) => match ExecutionResult::decode(message.payload) {
                                Ok(value) => {
                                    let execution_result = ExecutionLogonRequest {
                                        data: Some(Data::Response(value)),
                                    };

                                    if let Err(err) = tx.send(execution_result).await {
                                        log::error!(
                                            "Failed to send ExecutionLogonResponse: {:?}",
                                            err
                                        );
                                    }
                                }
                                Err(err) => {
                                    log::error!(
                                        "Failed to decode response from NATS server: {:?}",
                                        err
                                    );
                                }
                            },
                            Err(err) => {
                                log::error!("Failed to send request to NATS server: {:?}", err);
                            }
                        }
                    }
                }
                Err(status) => {
                    log::error!(
                        "Test execution stream error code={} message={}",
                        status.code(),
                        status.message()
                    );
                    break;
                }
            }
        }
    }
}
