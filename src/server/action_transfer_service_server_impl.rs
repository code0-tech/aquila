use crate::{
    configuration::service::ServiceConfiguration,
    sagittarius::module_service_client_impl::SagittariusModuleServiceClient,
};
use async_nats::{Subject, Subscriber};
use futures::StreamExt;
use futures_core::Stream;
use prost::Message;
use std::{collections::HashMap, pin::Pin, sync::Arc};
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;
use tucana::{
    aquila::{
        ActionEvent, ActionExecutionRequest, ActionExecutionResponse, ActionLogon,
        ActionTransferRequest, ActionTransferResponse,
        action_transfer_service_server::ActionTransferService,
    },
    shared::{ExecutionFlow, Flows, ValidationFlow, Value},
};

type PendingReplies = Arc<Mutex<HashMap<String, PendingReply>>>;

#[derive(Clone)]
struct PendingReply {
    reply_subject: Subject,
    keys: Vec<String>,
}

pub struct AquilaActionTransferServiceServer {
    client: async_nats::Client,
    kv: async_nats::jetstream::kv::Store,
    actions: ServiceConfiguration,
    module_service: Option<Arc<Mutex<SagittariusModuleServiceClient>>>,
    action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
    is_static: bool,
}

impl AquilaActionTransferServiceServer {
    pub fn new(
        client: async_nats::Client,
        kv: async_nats::jetstream::kv::Store,
        actions: ServiceConfiguration,
        module_service: Option<Arc<Mutex<SagittariusModuleServiceClient>>>,
        action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
        is_static: bool,
    ) -> Self {
        Self {
            client,
            kv,
            actions,
            module_service,
            action_config_tx,
            is_static,
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
    configs: &tucana::shared::ModuleConfigurations,
    action_identifier: &str,
) -> bool {
    configs.module_identifier == action_identifier
}

fn overwrite_module_definition_sources(
    module: &mut tucana::shared::Module,
    action_identifier: &str,
) {
    let source = format!("action.{}", action_identifier);

    for flow_type in &mut module.flow_types {
        flow_type.definition_source = Some(source.clone());
    }
    for runtime_flow_type in &mut module.runtime_flow_types {
        runtime_flow_type.definition_source = Some(source.clone());
    }
    for function_definition in &mut module.function_definitions {
        function_definition.definition_source = source.clone();
    }
    for runtime_function_definition in &mut module.runtime_function_definitions {
        runtime_function_definition.definition_source = source.clone();
    }
    for definition_data_type in &mut module.definition_data_types {
        definition_data_type.definition_source = source.clone();
    }
}

fn subject_execution_identifier(subject: &Subject) -> Option<String> {
    subject
        .as_str()
        .rsplit('.')
        .next()
        .filter(|execution_id| !execution_id.is_empty())
        .map(ToString::to_string)
}

fn pending_reply_keys(
    request_execution_id: &str,
    subject_execution_id: Option<&str>,
) -> Vec<String> {
    let mut keys = Vec::new();

    if !request_execution_id.is_empty() {
        keys.push(request_execution_id.to_string());
    }

    if let Some(subject_execution_id) = subject_execution_id {
        if !subject_execution_id.is_empty() && !keys.iter().any(|key| key == subject_execution_id) {
            keys.push(subject_execution_id.to_string());
        }
    }

    keys
}

fn insert_pending_reply(
    pending: &mut HashMap<String, PendingReply>,
    reply_subject: Subject,
    keys: Vec<String>,
) {
    let pending_reply = PendingReply {
        reply_subject,
        keys: keys.clone(),
    };

    for key in keys {
        pending.insert(key, pending_reply.clone());
    }
}

fn remove_pending_reply(
    pending: &mut HashMap<String, PendingReply>,
    execution_id: &str,
) -> Option<PendingReply> {
    let pending_reply = pending.remove(execution_id)?;

    for key in &pending_reply.keys {
        if key != execution_id {
            pending.remove(key);
        }
    }

    Some(pending_reply)
}

/// Extracts the bearer token from gRPC metadata.
fn extract_token(
    request: &tonic::Request<tonic::Streaming<ActionTransferRequest>>,
) -> Result<String, Status> {
    log::debug!("Extracting authorization token from metadata");
    match request.metadata().get("authorization") {
        Some(ascii) => match ascii.to_str() {
            Ok(tk) => {
                if tk.is_empty() {
                    log::error!("Authorization token is empty");
                    return Err(Status::unauthenticated("authorization token is empty"));
                }

                Ok(tk.to_string())
            }
            Err(err) => {
                log::error!("Cannot read authorization header because: {:?}", err);
                Err(Status::unauthenticated("invalid authorization header"))
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
    mut action_logon: ActionLogon,
    actions: Arc<Mutex<ServiceConfiguration>>,
    module_service: Option<Arc<Mutex<SagittariusModuleServiceClient>>>,
    client: async_nats::Client,
    cfg_tx: tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
    tx: tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
    pending_replies: PendingReplies,
    cfg_forwarder_started: &mut bool,
) -> Result<ActionLogon, Status> {
    log::info!("Action logon attempt payload={:?}", action_logon);

    let module = match action_logon.module.as_mut() {
        Some(m) => m,
        None => {
            return Err(Status::aborted("Please provide a module configuration."));
        }
    };
    let identifier = module.identifier.clone();

    {
        let lock = actions.lock().await;
        if !lock.has_action(&token.to_string(), &identifier) {
            log::warn!(
                "Rejected action logon identifier={} reason=token_not_registered",
                identifier
            );
            return Err(Status::unauthenticated(
                "token not matching to action identifier",
            ));
        }
    }

    overwrite_module_definition_sources(module, &identifier);

    if let Some(module_service) = module_service {
        let mut client = module_service.lock().await;
        let response = client
            .update_modules(tucana::aquila::ModuleUpdateRequest {
                modules: vec![module.clone()],
            })
            .await;

        if !response.success {
            log::error!(
                "Rejected action logon identifier={} reason=sagittarius_module_update_failed",
                identifier
            );
            return Err(Status::internal(
                "could not update action module via Sagittarius",
            ));
        }
    }

    log::debug!("Action connected identifier={}", identifier);

    let sub = match client.subscribe(format!("action.{}.*", identifier)).await {
        Ok(s) => s,
        Err(err) => {
            log::error!(
                "Could not subscribe to action: {}. Reason: {:?}",
                identifier,
                err
            );
            return Err(Status::internal(
                "could not register action into execution loop",
            ));
        }
    };

    if let Err(err) = client.flush().await {
        log::error!(
            "Could not flush action subscription: {}. Reason: {:?}",
            identifier,
            err
        );
        return Err(Status::internal(
            "could not register action subscription with NATS",
        ));
    }

    log::debug!("Subscribed to action subject action.{}.*", identifier);

    let tx_clone = tx.clone();
    let pending_replies_clone = pending_replies.clone();
    tokio::spawn(async move {
        forward_nats_to_action(sub, tx_clone, pending_replies_clone).await;
    });

    if !*cfg_forwarder_started {
        *cfg_forwarder_started = true;
        log::debug!("Starting config forwarder action={}", identifier);
        spawn_cfg_forwarder(identifier.clone(), cfg_tx, tx.clone());
    }

    Ok(action_logon)
}

/// Forwards config updates for the given action identifier to the gRPC stream.
fn spawn_cfg_forwarder(
    action_identifier: String,
    cfg_tx: tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
    tx: tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
) {
    let mut cfg_rx = cfg_tx.subscribe();
    tokio::spawn(async move {
        while let Ok(cfgs) = cfg_rx.recv().await {
            if !applies_to_action(&cfgs, &action_identifier) {
                log::debug!(
                    "Config update does not apply to action {}",
                    action_identifier
                );
                continue;
            }

            log::debug!("Forwarding config update to action {}", action_identifier);
            let resp = ActionTransferResponse {
                data: Some(
                    tucana::aquila::action_transfer_response::Data::ModuleConfigurations(cfgs),
                ),
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
    event: ActionEvent,
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

        log::info!("{:#?}", execution_flow);

        log::info!(
            "Requesting execution flow_id={} execution_id={}",
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

/// Publishes execution results back to the original NATS reply subject.
async fn handle_result(
    execution_result: ActionExecutionResponse,
    client: async_nats::Client,
    pending_replies: PendingReplies,
) {
    let execution_id = execution_result.execution_identifier.clone();

    let pending_reply = {
        let mut pending = pending_replies.lock().await;
        remove_pending_reply(&mut pending, &execution_id)
    };

    let Some(pending_reply) = pending_reply else {
        log::error!(
            "No pending NATS reply subject found execution_id={}",
            execution_id
        );
        return;
    };

    log::debug!(
        "Publishing execution result for {} to reply subject {}",
        execution_id,
        pending_reply.reply_subject
    );

    let payload = execution_result.encode_to_vec();
    if let Err(err) = client
        .publish(pending_reply.reply_subject, payload.into())
        .await
    {
        log::error!(
            "Failed to publish action result for execution {}: {:?}",
            execution_id,
            err
        );
        return;
    }

    if let Err(err) = client.flush().await {
        log::error!(
            "Failed to flush action result for execution {}: {:?}",
            execution_id,
            err
        );
    }
}

#[tonic::async_trait]
impl ActionTransferService for AquilaActionTransferServiceServer {
    type TransferStream =
        Pin<Box<dyn Stream<Item = Result<ActionTransferResponse, tonic::Status>> + Send + 'static>>;

    async fn transfer(
        &self,
        request: tonic::Request<tonic::Streaming<ActionTransferRequest>>,
    ) -> std::result::Result<tonic::Response<Self::TransferStream>, tonic::Status> {
        let token = extract_token(&request)?;
        log::debug!("Action transfer stream opened");

        let mut first_request = true;
        let mut action_props: Option<ActionLogon> = None;
        let mut stream = request.into_inner();

        let actions = Arc::new(Mutex::new(self.actions.clone()));
        let kv = self.kv.clone();
        let client = self.client.clone();
        let module_service = self.module_service.clone();
        let cfg_tx = self.action_config_tx.clone();
        let is_static = self.is_static;
        let pending_replies: PendingReplies = Arc::new(Mutex::new(HashMap::new()));

        let (tx, rx) =
            tokio::sync::mpsc::channel::<Result<ActionTransferResponse, tonic::Status>>(32);

        tokio::spawn(async move {
            let mut cfg_forwarder_started = false;
            log::debug!("Action transfer stream started");

            while let Some(next) = stream.next().await {
                let transfer_request = match next {
                    Ok(tr) => tr,
                    Err(status) => {
                        log::error!("Action transfer stream closed status={:?}", status);
                        break;
                    }
                };

                let data = match transfer_request.data {
                    Some(d) => d,
                    None => {
                        log::warn!("Received empty action transfer request");
                        continue;
                    }
                };

                if first_request {
                    first_request = false;

                    match data {
                        tucana::aquila::action_transfer_request::Data::Logon(action_logon) => {
                            let identifier = match action_logon.module {
                                Some(ref m) => m.identifier.clone(),
                                None => {
                                    log::error!("Logon failed (no module present)");
                                    break;
                                }
                            };

                            log::debug!("Received logon for action {}", identifier);

                            let accepted = match handle_logon(
                                &token,
                                action_logon,
                                actions.clone(),
                                module_service.clone(),
                                client.clone(),
                                cfg_tx.clone(),
                                tx.clone(),
                                pending_replies.clone(),
                                &mut cfg_forwarder_started,
                            )
                            .await
                            {
                                Ok(v) => v,
                                Err(status) => {
                                    log::error!("Action logon failed status={:?}", status);
                                    break;
                                }
                            };

                            action_props = Some(accepted);
                        }
                        _ => {
                            log::error!("Action stream protocol violation expected=logon");
                            break;
                        }
                    }

                    continue;
                }

                let props = match action_props.clone() {
                    Some(p) => p,
                    None => {
                        log::error!("Missing action properties after logon");
                        break;
                    }
                };

                let identifier = match props.module {
                    Some(ref m) => m.identifier.clone(),
                    None => {
                        log::error!("Logon state missing module");
                        break;
                    }
                };

                if is_static {
                    let lock = actions.lock().await;
                    let configs = lock.get_action_configuration(&identifier);
                    for conf in configs {
                        if let Err(err) = cfg_tx.send(conf) {
                            log::warn!("No action configuration receivers available: {:?}", err);
                        }
                    }
                };

                match data {
                    tucana::aquila::action_transfer_request::Data::Logon(_) => {
                        log::error!("Received duplicate logon");
                        break;
                    }
                    tucana::aquila::action_transfer_request::Data::Event(event) => {
                        log::debug!("Received event action={}", identifier);
                        handle_event(event, kv.clone(), client.clone()).await;
                    }
                    tucana::aquila::action_transfer_request::Data::Result(execution_result) => {
                        log::debug!(
                            "Received execution result execution_id={} action={}",
                            execution_result.execution_identifier,
                            identifier
                        );

                        handle_result(execution_result, client.clone(), pending_replies.clone())
                            .await;
                    }
                }
            }

            log::debug!("Action transfer stream ended");
        });

        Ok(tonic::Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

/// Forwards NATS execution requests to the connected action via gRPC and stores reply subjects.
async fn forward_nats_to_action(
    mut sub: Subscriber,
    tx: tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
    pending_replies: PendingReplies,
) {
    log::debug!("Waiting for incoming action execution request");

    while let Some(msg) = sub.next().await {
        log::debug!("Received RemoteRuntime execution request");

        let mut execution = match ActionExecutionRequest::decode(msg.payload.as_ref()) {
            Ok(req) => req,
            Err(err) => {
                log::error!("Invalid execution request payload: {:?}", err);
                continue;
            }
        };

        let subject_execution_id = subject_execution_identifier(&msg.subject);
        if execution.execution_identifier.is_empty() {
            if let Some(subject_execution_id) = subject_execution_id.as_ref() {
                log::warn!(
                    "Filled missing action execution identifier from NATS subject subject={} execution_id={}",
                    msg.subject,
                    subject_execution_id
                );
                execution.execution_identifier = subject_execution_id.clone();
            }
        }

        let execution_id = execution.execution_identifier.clone();

        let Some(reply_subject) = msg.reply.clone() else {
            log::error!(
                "Received request without NATS reply subject execution_id={}",
                execution_id
            );
            continue;
        };

        let keys = pending_reply_keys(&execution_id, subject_execution_id.as_deref());
        if keys.is_empty() {
            log::error!(
                "Cannot store NATS reply subject without execution identifier subject={} reply_subject={}",
                msg.subject,
                reply_subject
            );
            continue;
        }

        {
            let mut pending = pending_replies.lock().await;
            insert_pending_reply(&mut pending, reply_subject.clone(), keys.clone());
        }

        log::debug!(
            "Stored reply subject reply_subject={} execution_id={} keys={:?}",
            reply_subject,
            execution_id,
            keys
        );

        log::debug!(
            "Forwarding execution request to action execution_id={} request={:#?}",
            execution_id,
            execution
        );

        let resp = ActionTransferResponse {
            data: Some(tucana::aquila::action_transfer_response::Data::Execution(
                execution,
            )),
        };

        if tx.send(Ok(resp)).await.is_err() {
            log::debug!("Execution forwarder channel closed");

            // cleanup, since the request can no longer be delivered to the action
            let mut pending = pending_replies.lock().await;
            remove_pending_reply(&mut pending, &execution_id);

            break;
        }
    }

    log::debug!("Execution forwarder stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_configurations_apply_by_module_identifier() {
        let configs = tucana::shared::ModuleConfigurations {
            module_identifier: "gls-action".to_string(),
            module_configurations: vec![tucana::shared::ModuleProjectConfigurations {
                project_id: 1,
                module_configurations: vec![tucana::shared::ModuleConfiguration {
                    identifier: "username".to_string(),
                    value: None,
                }],
            }],
        };

        assert!(applies_to_action(&configs, "gls-action"));
        assert!(!applies_to_action(&configs, "another-action"));
    }

    #[test]
    fn overwrite_module_definition_sources_uses_action_source() {
        let mut module = tucana::shared::Module {
            flow_types: vec![tucana::shared::FlowType {
                definition_source: Some("module.old".to_string()),
                ..Default::default()
            }],
            runtime_flow_types: vec![tucana::shared::RuntimeFlowType {
                definition_source: Some("module.old".to_string()),
                ..Default::default()
            }],
            function_definitions: vec![tucana::shared::FunctionDefinition {
                definition_source: "module.old".to_string(),
                ..Default::default()
            }],
            runtime_function_definitions: vec![tucana::shared::RuntimeFunctionDefinition {
                definition_source: "module.old".to_string(),
                ..Default::default()
            }],
            definition_data_types: vec![tucana::shared::DefinitionDataType {
                definition_source: "module.old".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        overwrite_module_definition_sources(&mut module, "send-email");

        assert_eq!(
            module.flow_types[0].definition_source.as_deref(),
            Some("action.send-email")
        );
        assert_eq!(
            module.runtime_flow_types[0].definition_source.as_deref(),
            Some("action.send-email")
        );
        assert_eq!(
            module.function_definitions[0].definition_source,
            "action.send-email"
        );
        assert_eq!(
            module.runtime_function_definitions[0].definition_source,
            "action.send-email"
        );
        assert_eq!(
            module.definition_data_types[0].definition_source,
            "action.send-email"
        );
    }

    #[test]
    fn pending_reply_keys_include_payload_and_subject_ids_once() {
        assert_eq!(
            pending_reply_keys("payload-id", Some("subject-id")),
            vec!["payload-id".to_string(), "subject-id".to_string()]
        );
        assert_eq!(
            pending_reply_keys("same-id", Some("same-id")),
            vec!["same-id".to_string()]
        );
        assert_eq!(
            pending_reply_keys("", Some("subject-id")),
            vec!["subject-id".to_string()]
        );
    }

    #[test]
    fn remove_pending_reply_removes_all_aliases() {
        let reply_subject = Subject::from("_INBOX.reply");
        let mut pending = HashMap::new();

        insert_pending_reply(
            &mut pending,
            reply_subject.clone(),
            vec!["payload-id".to_string(), "subject-id".to_string()],
        );

        let removed = remove_pending_reply(&mut pending, "subject-id")
            .expect("pending reply should be found by alias");

        assert_eq!(removed.reply_subject, reply_subject);
        assert!(pending.is_empty());
    }

    #[test]
    fn subject_execution_identifier_uses_last_subject_token() {
        assert_eq!(
            subject_execution_identifier(&Subject::from("action.example.execution-id")),
            Some("execution-id".to_string())
        );
    }
}
