//! Handles the first message of an action's transfer stream: authenticating
//! the token, registering the action's module with Sagittarius, and wiring
//! up the NATS subscriptions that feed the rest of the stream.

use tonic::Status;
use tucana::aquila::{ActionFlowUpdate, ActionLogon, ActionTransferResponse};

use crate::{
    flow::{FlowChange, flow_belongs_to_action, to_action_flow},
    telemetry::{errors, metrics},
};

use super::{
    ActionTransferContext,
    nats_bridge::{forward_nats_to_action, get_flows},
    pending_replies::PendingReplyStore,
};

/// Extracts the bearer token from gRPC metadata.
pub(super) fn extract_token(
    request: &tonic::Request<tonic::Streaming<tucana::aquila::ActionTransferRequest>>,
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

/// Whether a broadcasted config update is meant for `action_identifier`, since
/// [`spawn_cfg_forwarder`] subscribes to a single broadcast channel shared by
/// every connected action.
fn applies_to_action(
    configs: &tucana::shared::ModuleConfigurations,
    action_identifier: &str,
) -> bool {
    configs.module_identifier == action_identifier
}

/// Rewrites every definition's `definition_source` on the module an action
/// logs on with, so downstream consumers can tell it came from this action
/// rather than from whatever source the action's module definition was
/// authored against.
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

/// Validates the logon request, starts NATS + config/flow forwarders, and returns the accepted logon.
pub(super) async fn handle_logon(
    token: &str,
    mut action_logon: ActionLogon,
    context: ActionTransferContext,
    tx: tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
    pending_replies: PendingReplyStore,
    cfg_forwarder_started: &mut bool,
    flow_forwarder_started: &mut bool,
) -> Result<ActionLogon, Status> {
    let module = match action_logon.module.as_mut() {
        Some(m) => m,
        None => {
            log::warn!("Rejected action logon reason=missing_module");
            return Err(Status::aborted("Please provide a module configuration."));
        }
    };
    let identifier = module.identifier.clone();
    log::info!("Action logon attempt identifier={}", identifier);

    if !context.actions.has_action(&token.to_string(), &identifier) {
        metrics::action_connection(&identifier, "rejected");
        metrics::action_failure(&identifier, "authentication");
        log::warn!(
            "Rejected action logon identifier={} reason=token_not_registered",
            identifier
        );
        return Err(Status::unauthenticated(
            "token not matching to action identifier",
        ));
    }

    overwrite_module_definition_sources(module, &identifier);

    if let Some(module_service) = &context.module_service {
        let available_definition_sources = context.actions.collect_modules();
        let mut client = module_service.lock().await;
        let response = client
            .update_modules(
                tucana::aquila::ModuleUpdateRequest {
                    modules: vec![module.clone()],
                },
                available_definition_sources,
            )
            .await;

        if !response.success {
            metrics::action_connection(&identifier, "rejected");
            metrics::action_failure(&identifier, "module_update");
            errors::record_message(
                "dependency",
                "action.logon",
                "Sagittarius rejected the action module update",
                format!("action.identifier={identifier}"),
            );
            return Err(Status::internal(
                "could not update action module via Sagittarius",
            ));
        }
    }

    log::debug!("Action connected identifier={}", identifier);

    let sub = match context
        .client
        .subscribe(format!("action.{}.*", identifier))
        .await
    {
        Ok(s) => s,
        Err(err) => {
            metrics::action_connection(&identifier, "rejected");
            metrics::action_failure(&identifier, "subscription");
            errors::record(
                "messaging",
                "action.subscribe",
                &err,
                format!("action.identifier={identifier} subject=action.{identifier}.*"),
            );
            return Err(Status::internal(
                "could not register action into execution loop",
            ));
        }
    };

    if let Err(err) = context.client.flush().await {
        metrics::action_connection(&identifier, "rejected");
        metrics::action_failure(&identifier, "subscription_flush");
        errors::record(
            "messaging",
            "action.subscribe.flush",
            &err,
            format!("action.identifier={identifier}"),
        );
        return Err(Status::internal(
            "could not register action subscription with NATS",
        ));
    }

    log::debug!("Subscribed to action subject action.{}.*", identifier);

    let tx_clone = tx.clone();
    let pending_replies_clone = pending_replies.clone();
    let forwarder_identifier = identifier.clone();
    tokio::spawn(async move {
        forward_nats_to_action(forwarder_identifier, sub, tx_clone, pending_replies_clone).await;
    });

    // A logon is only the first message on the stream, but `handle_logon` can't
    // assume it's only ever called once per stream, so the caller-owned flag
    // guards against starting a second, redundant forwarder task.
    if !*cfg_forwarder_started {
        *cfg_forwarder_started = true;
        log::debug!("Starting config forwarder action={}", identifier);
        spawn_cfg_forwarder(
            identifier.clone(),
            context.action_config_tx.clone(),
            tx.clone(),
        );
    }

    if !*flow_forwarder_started {
        *flow_forwarder_started = true;
        send_known_flows(&identifier, context.kv.clone(), tx.clone()).await;
        log::debug!("Starting flow forwarder action={}", identifier);
        spawn_flow_forwarder(
            identifier.clone(),
            context.action_flow_tx.clone(),
            tx.clone(),
        );
    }

    Ok(action_logon)
}

/// Sends every flow this action already owns from the flow store, so a
/// newly connected action doesn't have to wait for its next update to learn
/// about flows created before it connected.
async fn send_known_flows(
    action_identifier: &str,
    kv: async_nats::jetstream::kv::Store,
    tx: tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
) {
    let flows = match get_flows("*.*.*.*".to_string(), kv).await {
        Ok(flows) => flows,
        Err(err) => {
            errors::record(
                "flow_storage",
                "action.logon.known_flows",
                &err,
                format!("action.identifier={action_identifier}"),
            );
            return;
        }
    };

    let mut sent_count = 0;
    for flow in flows.flows {
        if !flow_belongs_to_action(&flow, action_identifier) {
            continue;
        }

        let resp = ActionTransferResponse {
            data: Some(tucana::aquila::action_transfer_response::Data::FlowUpdate(
                ActionFlowUpdate {
                    data: Some(tucana::aquila::action_flow_update::Data::UpdatedFlow(
                        to_action_flow(&flow),
                    )),
                },
            )),
        };

        if tx.send(Ok(resp)).await.is_err() {
            log::debug!(
                "Action transfer response stream closed while sending known flows action={}",
                action_identifier
            );
            return;
        }
        sent_count += 1;
    }

    log::debug!(
        "Sent known flows to action action={} flow_count={}",
        action_identifier,
        sent_count
    );
}

/// Forwards config updates for the given action identifier to the gRPC stream.
pub(super) fn spawn_cfg_forwarder(
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
                metrics::action_config_update(&action_identifier, "failed");
                metrics::action_failure(&action_identifier, "configuration_forward");
                log::debug!("Config forwarder channel closed for {}", action_identifier);
                break;
            }
            metrics::action_config_update(&action_identifier, "success");
        }

        log::debug!("Config forwarder stopped for {}", action_identifier);
    });
}

/// Forwards flow store changes that belong to the given action identifier to the gRPC stream.
pub(super) fn spawn_flow_forwarder(
    action_identifier: String,
    flow_tx: tokio::sync::broadcast::Sender<FlowChange>,
    tx: tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>,
) {
    let mut flow_rx = flow_tx.subscribe();
    tokio::spawn(async move {
        while let Ok(change) = flow_rx.recv().await {
            let data = match change {
                FlowChange::Updated(flow) => {
                    if !flow_belongs_to_action(&flow, &action_identifier) {
                        continue;
                    }
                    tucana::aquila::action_flow_update::Data::UpdatedFlow(to_action_flow(&flow))
                }
                FlowChange::Deleted {
                    flow_id,
                    definition_source,
                } => {
                    if definition_source != format!("action.{action_identifier}") {
                        continue;
                    }
                    tucana::aquila::action_flow_update::Data::DeletedFlow(flow_id)
                }
            };

            log::debug!("Forwarding flow update to action {}", action_identifier);
            let resp = ActionTransferResponse {
                data: Some(tucana::aquila::action_transfer_response::Data::FlowUpdate(
                    ActionFlowUpdate { data: Some(data) },
                )),
            };

            if tx.send(Ok(resp)).await.is_err() {
                metrics::flow_operation("forward", "failure", 1);
                metrics::action_failure(&action_identifier, "flow_forward");
                log::debug!("Flow forwarder channel closed for {}", action_identifier);
                break;
            }
            metrics::flow_operation("forward", "success", 1);
        }

        log::debug!("Flow forwarder stopped for {}", action_identifier);
    });
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
}
