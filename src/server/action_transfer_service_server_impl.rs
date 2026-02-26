use crate::configuration::action::ActionConfiguration;
use async_nats::Subscriber;
use futures::StreamExt;
use futures_core::Stream;
use prost::Message;
use std::{pin::Pin, sync::Arc};
use tokio::sync::Mutex;
use tonic::Status;
use tucana::{
    aquila::{
        ActionLogon, TransferRequest, TransferResponse,
        action_transfer_service_server::ActionTransferService,
    },
    shared::{ExecutionFlow, Flows, ValidationFlow, Value},
};

pub struct AquilaActionTransferServiceServer {
    client: async_nats::Client,
    kv: async_nats::jetstream::kv::Store,
    actions: ActionConfiguration,
}

impl AquilaActionTransferServiceServer {
    pub fn new(
        client: async_nats::Client,
        kv: async_nats::jetstream::kv::Store,
        actions: ActionConfiguration,
    ) -> Self {
        Self {
            client,
            kv,
            actions,
        }
    }
}

enum FlowIdentificationError {
    KVError,
}

async fn get_flows(
    pattern: String,
    kv: async_nats::jetstream::kv::Store,
) -> Result<Flows, FlowIdentificationError> {
    let mut collector = Vec::new();
    let mut keys = match kv.keys().await {
        Ok(keys) => keys.boxed(),
        Err(err) => {
            log::error!("Failed to get keys: {:?}", err);
            return Err(FlowIdentificationError::KVError);
        }
    };

    while let Ok(Some(key)) = tokio_stream::StreamExt::try_next(&mut keys).await {
        if !is_matching_key(&pattern, &key) {
            continue;
        }

        if let Ok(Some(bytes)) = kv.get(key).await {
            let decoded_flow = ValidationFlow::decode(bytes);
            if let Ok(flow) = decoded_flow {
                collector.push(flow.clone());
            };
        }
    }
    Ok(Flows { flows: collector })
}

fn is_matching_key(pattern: &String, key: &String) -> bool {
    let split_pattern = pattern.split(".");
    let split_key = key.split(".").collect::<Vec<&str>>();
    let zip = split_pattern.into_iter().zip(split_key);

    for (pattern_part, key_part) in zip {
        if pattern_part == "*" {
            continue;
        }

        if pattern_part != key_part {
            return false;
        }
    }
    true
}

fn convert_validation_flow(flow: ValidationFlow, input_value: Option<Value>) -> ExecutionFlow {
    ExecutionFlow {
        flow_id: flow.flow_id,
        starting_node_id: flow.starting_node_id,
        input_value,
        node_functions: flow.node_functions,
    }
}
//TODO: Aquila needs to listen to taurus exection requests and then send it to the action
#[tonic::async_trait]
impl ActionTransferService for AquilaActionTransferServiceServer {
    type TransferStream =
        Pin<Box<dyn Stream<Item = Result<TransferResponse, tonic::Status>> + Send + 'static>>;

    async fn transfer(
        &self,
        request: tonic::Request<tonic::Streaming<TransferRequest>>,
    ) -> std::result::Result<tonic::Response<Self::TransferStream>, tonic::Status> {
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

        let mut first_request = true;
        let mut action_props: Option<ActionLogon> = None;
        let mut stream = request.into_inner();

        let actions = Arc::new(Mutex::new(self.actions.clone()));
        let kv = self.kv.clone();
        let client = self.client.clone();

        let mut sub: Option<Subscriber> = None;

        tokio::spawn(async move {
            while let Some(next) = stream.next().await {
                let transfer_request = match next {
                    Ok(tr) => tr,
                    Err(status) => {
                        log::error!("Stream was closed with status code: {:?}", status);
                        break;
                    }
                };

                let data = match transfer_request.data {
                    Some(d) => d,
                    None => {
                        log::error!("Recieved empty request, waiting on next one");
                        continue;
                    }
                };

                // The first request needs to be an ActionLogon request to get the serive name
                // If its not an ActionLogon request the connection is abborted
                if first_request {
                    first_request = false;

                    match data {
                        tucana::aquila::transfer_request::Data::Logon(action_logon) => {
                            log::info!("Action successfull logged on: {:?}", action_logon);
                            let lock = actions.lock().await;
                            match lock.has_action(&token, &action_logon.action_identifier) {
                                true => {
                                    log::debug!(
                                        "Action with identifer: {}, connected successfully",
                                        action_logon.action_identifier
                                    );
                                }
                                false => {
                                    log::debug!(
                                        "Rejected action with identifer: {}, becuase its not registered",
                                        action_logon.action_identifier
                                    );
                                    return Err(Status::unauthenticated(
                                        "token not matching to action identifier",
                                    ));
                                }
                            }

                            action_props = Some(action_logon.clone());
                            sub = match client
                                .subscribe(format!("action.{}.*", action_logon.action_identifier))
                                .await
                            {
                                Ok(s) => Some(s),
                                Err(err) => {
                                    log::error!(
                                        "Cound not subscribe to action: {}. Reason: {:?}",
                                        action_logon.action_identifier,
                                        err
                                    );
                                    return Err(Status::internal(
                                        "cound not register action into execution loop",
                                    ));
                                }
                            };
                        }
                        _ => {
                            log::error!(
                                "Action tried to logon but was not sending a logon request!"
                            );
                            return Err(Status::internal(
                                "First request needs to be a 'ActionLogonRequest'",
                            ));
                        }
                    }
                    continue;
                }

                let props = match action_props {
                    Some(ref p) => p.clone(),
                    None => return Err(Status::internal("Missing actions informations")),
                };

                match data {
                    tucana::aquila::transfer_request::Data::Logon(_action_logon) => {
                        return Err(Status::internal(
                            "Already logged on. Send 'Logon' request only once",
                        ));
                    }
                    tucana::aquila::transfer_request::Data::Event(event) => {
                        //TODO get project id
                        let pattern = format!("{}.*.{}.*", event.event_type, "abc");
                        let flows = match get_flows(pattern, kv.clone()).await {
                            Ok(f) => f,
                            Err(_) => {
                                log::error!("Cound not find any flows");
                                continue;
                            }
                        };

                        for flow in flows.flows {
                            let uuid = uuid::Uuid::new_v4().to_string();
                            let flow_id = flow.flow_id;
                            let execution_flow: ExecutionFlow =
                                convert_validation_flow(flow, event.payload.clone());
                            let bytes = execution_flow.encode_to_vec();
                            let topic = format!("execution.{}", uuid);
                            log::info!(
                                "Requesting execution of flow {} with execution id {}",
                                flow_id,
                                uuid
                            );
                            let _ = client.request(topic, bytes.into()).await;
                        }
                    }
                    tucana::aquila::transfer_request::Data::Result(execution_result) => {
                        todo!("respond into nats with result")
                    }
                }
            }
            Ok(())
        });
        unimplemented!()
    }
}
