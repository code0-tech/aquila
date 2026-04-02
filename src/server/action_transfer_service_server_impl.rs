use crate::configuration::action::ActionConfiguration;
use async_nats::Subscriber;
use futures::StreamExt;
use futures_core::Stream;
use prost::Message;
use std::{pin::Pin, sync::Arc};
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;
use tucana::{
    aquila::{
        ActionLogon, Event, ExecutionRequest, ExecutionResult, TransferRequest, TransferResponse,
        action_transfer_service_server::ActionTransferService, transfer_response,
    },
    shared::{ExecutionFlow, Flows, ValidationFlow, Value},
};

pub struct AquilaActionTransferServiceServer {
    client: async_nats::Client,
    kv: async_nats::jetstream::kv::Store,
    actions: ActionConfiguration,
    action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ActionConfigurations>,
}

impl AquilaActionTransferServiceServer {
    pub fn new(
        client: async_nats::Client,
        kv: async_nats::jetstream::kv::Store,
        actions: ActionConfiguration,
        action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ActionConfigurations>,
    ) -> Self {
        Self {
            client,
            kv,
            actions,
            action_config_tx,
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
    log::debug!("Scanning flows with pattern: {}", pattern);
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

        match kv.get(key.clone()).await {
            Ok(Some(bytes)) => {
                let decoded_flow = ValidationFlow::decode(bytes);
                match decoded_flow {
                    Ok(flow) => collector.push(flow),
                    Err(err) => {
                        log::error!("Failed to decode flow {}: {:?}", key, err);
                    }
                }
            }
            Ok(None) => {
                log::debug!("Flow key disappeared while reading: {}", key);
            }
            Err(err) => {
                log::error!("Failed to fetch flow {}: {:?}", key, err);
            }
        }
    }
    log::debug!("Matched {} flows for pattern {}", collector.len(), pattern);
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
            log::debug!("Key {} does not match pattern {}", key, pattern);
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
        project_id: flow.project_id,
    }
}

fn applies_to_action(
    configs: &tucana::shared::ActionConfigurations,
    action_identifier: &str,
) -> bool {
    configs.action_configurations.iter().any(|project_cfg| {
        project_cfg
            .action_configurations
            .iter()
            .any(|cfg| cfg.identifier == action_identifier)
    })
}

/// Extracts the bearer token from gRPC metadata.
fn extract_token(
    request: &tonic::Request<tonic::Streaming<TransferRequest>>,
) -> Result<String, Status> {
    log::debug!("Extracting authorization token from metadata");
    match request.metadata().get("authorization") {
        Some(ascii) => match ascii.to_str() {
            Ok(tk) => Ok(tk.to_string()),
            Err(err) => {
                log::error!("Cannot read authorization header because: {:?}", err);
                Err(Status::internal("cannot read authorization header"))
            }
        },
        None => {
            log::error!("Missing authorization token");
            Err(Status::unauthenticated("missing authorization token"))
        }
    }
}

/// Validates the logon request, starts NATS + config forwarders, and returns the accepted logon.
async fn handle_logon(
    token: &str,
    action_logon: ActionLogon,
    actions: Arc<Mutex<ActionConfiguration>>,
    client: async_nats::Client,
    cfg_tx: tokio::sync::broadcast::Sender<tucana::shared::ActionConfigurations>,
    tx: tokio::sync::mpsc::Sender<Result<TransferResponse, tonic::Status>>,
    cfg_forwarder_started: &mut bool,
) -> Result<ActionLogon, Status> {
    log::info!("Action successfull logged on: {:?}", action_logon);
    let lock = actions.lock().await;
    if !lock.has_action(&token.to_string(), &action_logon.action_identifier) {
        log::debug!(
            "Rejected action with identifer: {}, becuase its not registered",
            action_logon.action_identifier
        );
        return Err(Status::unauthenticated(
            "token not matching to action identifier",
        ));
    }

    log::debug!(
        "Action with identifer: {}, connected successfully",
        action_logon.action_identifier
    );

    let sub = match client
        .subscribe(format!("action.{}.*", action_logon.action_identifier))
        .await
    {
        Ok(s) => s,
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
    log::debug!(
        "Subscribed to action subject action.{}.*",
        action_logon.action_identifier
    );
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        forward_nats_to_action(sub, tx_clone).await;
    });

    if !*cfg_forwarder_started {
        *cfg_forwarder_started = true;
        log::debug!(
            "Starting config forwarder for action {}",
            action_logon.action_identifier
        );
        spawn_cfg_forwarder(action_logon.action_identifier.clone(), cfg_tx, tx.clone());
    }

    Ok(action_logon)
}

/// Forwards config updates for the given action identifier to the gRPC stream.
fn spawn_cfg_forwarder(
    action_identifier: String,
    cfg_tx: tokio::sync::broadcast::Sender<tucana::shared::ActionConfigurations>,
    tx: tokio::sync::mpsc::Sender<Result<TransferResponse, tonic::Status>>,
) {
    let mut cfg_rx = cfg_tx.subscribe();
    tokio::spawn(async move {
        while let Ok(cfgs) = cfg_rx.recv().await {
            // TODO: Replace incoming identifier with the correct action identifier.
            if !applies_to_action(&cfgs, &action_identifier) {
                log::debug!(
                    "Config update does not apply to action {}",
                    action_identifier
                );
                continue;
            }
            log::debug!("Forwarding config update to action {}", action_identifier);
            let resp = TransferResponse {
                data: Some(transfer_response::Data::ActionConfigurations(cfgs)),
            };
            if tx.send(Ok(resp)).await.is_err() {
                log::debug!("Config forwarder channel closed for {}", action_identifier);
                break;
            }
        }
        log::debug!("Config forwarder stopped for {}", action_identifier);
    });
}

/// Looks up matching flows for an event and requests their execution.
async fn handle_event(
    event: Event,
    kv: async_nats::jetstream::kv::Store,
    client: async_nats::Client,
) {
    let pattern = format!("{}.*.{}.*", event.event_type, event.project_id);
    log::debug!(
        "Handling event type {} for project {}",
        event.event_type,
        event.project_id
    );
    let flows = match get_flows(pattern, kv).await {
        Ok(f) => f,
        Err(_) => {
            log::error!("Cound not find any flows");
            return;
        }
    };

    for flow in flows.flows {
        let uuid = uuid::Uuid::new_v4().to_string();
        let flow_id = flow.flow_id;
        let execution_flow: ExecutionFlow = convert_validation_flow(flow, event.payload.clone());
        let bytes = execution_flow.encode_to_vec();
        let topic = format!("execution.{}", uuid);
        log::info!(
            "Requesting execution of flow {} with execution id {}",
            flow_id,
            uuid
        );
        if let Err(err) = client.request(topic, bytes.into()).await {
            log::error!(
                "Failed to request execution for flow {}: {:?}",
                flow_id,
                err
            );
        }
    }
}

/// Publishes execution results back to NATS for the waiting requester.
async fn handle_result(
    action_identifier: &str,
    execution_result: ExecutionResult,
    client: async_nats::Client,
) {
    let topic = format!(
        "action.{}.{}",
        action_identifier, execution_result.execution_identifier
    );
    log::debug!("Publishing execution result to {}", topic);
    let payload = execution_result.encode_to_vec();
    if let Err(err) = client.publish(topic, payload.into()).await {
        log::error!("Failed to publish action result: {:?}", err);
    }
    todo!("respond into nats with result")
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
        let token = extract_token(&request)?;

        let mut first_request = true;
        let mut action_props: Option<ActionLogon> = None;
        let mut stream = request.into_inner();

        let actions = Arc::new(Mutex::new(self.actions.clone()));
        let kv = self.kv.clone();
        let client = self.client.clone();
        let cfg_tx = self.action_config_tx.clone();

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<TransferResponse, tonic::Status>>(32);

        tokio::spawn(async move {
            let mut cfg_forwarder_started = false;
            log::debug!("Action transfer stream started");

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
                            log::debug!(
                                "Received logon for action {}",
                                action_logon.action_identifier
                            );
                            let accepted = handle_logon(
                                &token,
                                action_logon,
                                actions.clone(),
                                client.clone(),
                                cfg_tx.clone(),
                                tx.clone(),
                                &mut cfg_forwarder_started,
                            )
                            .await?;
                            action_props = Some(accepted);
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
                    None => {
                        log::error!("Missing action properties after logon");
                        return Err(Status::internal("Missing actions informations"));
                    }
                };

                match data {
                    tucana::aquila::transfer_request::Data::Logon(_action_logon) => {
                        log::error!("Received duplicate logon after initial logon");
                        return Err(Status::internal(
                            "Already logged on. Send 'Logon' request only once",
                        ));
                    }
                    tucana::aquila::transfer_request::Data::Event(event) => {
                        log::debug!("Received event from action {}", props.action_identifier);
                        handle_event(event, kv.clone(), client.clone()).await;
                    }
                    tucana::aquila::transfer_request::Data::Result(execution_result) => {
                        log::debug!(
                            "Received execution result {} from action {}",
                            execution_result.execution_identifier,
                            props.action_identifier
                        );
                        handle_result(&props.action_identifier, execution_result, client.clone())
                            .await;
                    }
                }
            }
            log::debug!("Action transfer stream ended");
            Ok(())
        });
        Ok(tonic::Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

/// Forwards NATS execution requests to the connected action via gRPC.
async fn forward_nats_to_action(
    mut sub: Subscriber,
    tx: tokio::sync::mpsc::Sender<Result<TransferResponse, tonic::Status>>,
) {
    while let Some(msg) = sub.next().await {
        let execution = match ExecutionRequest::decode(msg.payload.as_ref()) {
            Ok(req) => req,
            Err(err) => {
                log::error!("Invalid execution request payload: {:?}", err);
                continue;
            }
        };
        let resp = TransferResponse {
            data: Some(transfer_response::Data::Execution(execution)),
        };
        if tx.send(Ok(resp)).await.is_err() {
            log::debug!("Execution forwarder channel closed");
            break;
        }
    }
    log::debug!("Execution forwarder stopped");
}
