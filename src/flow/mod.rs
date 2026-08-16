//! The key scheme Aquila's flow KV store uses, and the two operations built
//! on it: computing a flow's key and checking whether a key belongs to a
//! given flow id. There is no secondary index, so every "find by flow id"
//! lookup elsewhere in the codebase is a scan using [`key_has_flow_id`].

use futures::TryStreamExt;
use prost::Message;
use tucana::aquila::ActionFlow;
use tucana::shared::{ExecutionFlow, ValidationFlow};

/// Every flow identifier has this key
/// `<type>.<project_slug>.<project_id>.<flow_id>`
pub fn get_flow_identifier(flow: &ValidationFlow) -> String {
    format!(
        "{}.{}.{}.{}",
        flow.r#type, flow.project_slug, flow.project_id, flow.flow_id
    )
}

pub fn key_has_flow_id(key: &str, flow_id: i64) -> bool {
    key.rsplit_once('.')
        .and_then(|(_, id)| id.parse::<i64>().ok())
        == Some(flow_id)
}

/// Whether `flow` was created against the module an action logs on with -
/// i.e. whether its `definition_source` is the `"action.<identifier>"` value
/// `overwrite_module_definition_sources` stamps onto that action's module.
pub fn flow_belongs_to_action(flow: &ValidationFlow, action_identifier: &str) -> bool {
    let source = format!("action.{action_identifier}");
    flow.definition_source.as_deref() == Some(source.as_str())
}

/// Projects a `ValidationFlow` down to the fields an action needs to
/// execute/validate against it - an "action flow".
pub fn to_action_flow(flow: &ValidationFlow) -> ActionFlow {
    ActionFlow {
        flow_id: flow.flow_id,
        project_id: flow.project_id,
        settings: flow.settings.clone(),
        input_schema: flow.input_schema.clone(),
        output_schema: flow.output_schema.clone(),
        project_slug: flow.project_slug.clone(),
        name: flow.name.clone(),
        r#type: flow.r#type.clone(),
    }
}

/// Scans the flow KV bucket for the entry whose key encodes `flow_id`. There
/// is no secondary index from flow id to key, so this is a linear scan of
/// every stored flow - shared by every caller that needs to resolve a flow
/// by id alone (Sagittarius test executions, action-triggered executions).
pub async fn load_validation_flow_by_id(
    store: &async_nats::jetstream::kv::Store,
    flow_id: i64,
) -> Option<ValidationFlow> {
    let mut keys = match store.keys().await {
        Ok(keys) => keys,
        Err(err) => {
            log::error!(
                "Failed to list validation flow keys flow_id={} error={:?}",
                flow_id,
                err
            );
            return None;
        }
    };

    let key = loop {
        match keys.try_next().await {
            Ok(Some(key)) if key_has_flow_id(&key, flow_id) => break key,
            Ok(Some(_)) => {}
            Ok(None) => {
                log::error!("Validation flow was not found flow_id={}", flow_id);
                return None;
            }
            Err(err) => {
                log::error!(
                    "Failed while scanning validation flow keys flow_id={} error={:?}",
                    flow_id,
                    err
                );
                return None;
            }
        }
    };

    match store.get(&key).await {
        Ok(Some(bytes)) => match ValidationFlow::decode(bytes) {
            Ok(flow) => Some(flow),
            Err(err) => {
                log::error!(
                    "Failed to decode validation flow flow_id={} error={:?}",
                    flow_id,
                    err
                );
                None
            }
        },
        Ok(None) => {
            log::error!(
                "Validation flow disappeared after key resolution flow_id={} key={}",
                flow_id,
                key
            );
            None
        }
        Err(err) => {
            log::error!(
                "Failed to fetch validation flow flow_id={} key={} error={:?}",
                flow_id,
                key,
                err
            );
            None
        }
    }
}

/// Publishes `execution_flow` onto the NATS execution bus under
/// `execution.<execution_id>`, the subject a runtime picks the request up
/// from - shared by every execution source (Sagittarius test executions,
/// action-triggered executions).
pub async fn dispatch_execution(
    nats_client: &async_nats::Client,
    execution_id: &str,
    execution_flow: &ExecutionFlow,
) -> Result<(), async_nats::PublishError> {
    let bytes = execution_flow.encode_to_vec();
    let topic = format!("execution.{}", execution_id);
    nats_client.publish(topic, bytes.into()).await
}

/// A change to the flow store, broadcast so every connected action can keep
/// its own view of the flows it owns in sync - see [`flow_belongs_to_action`].
#[derive(Clone, Debug)]
pub enum FlowChange {
    Updated(Box<ValidationFlow>),
    Deleted {
        flow_id: i64,
        definition_source: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_flow_id_in_final_key_segment() {
        assert!(key_has_flow_id("CRON.test.1.1", 1));
        assert!(key_has_flow_id("REST.project.42.123", 123));
    }

    #[test]
    fn rejects_partial_or_non_final_flow_id_matches() {
        assert!(!key_has_flow_id("CRON.test.1.11", 1));
        assert!(!key_has_flow_id("CRON.test.1.1.extra", 1));
        assert!(!key_has_flow_id("CRON.test.1.invalid", 1));
    }

    fn flow(definition_source: Option<&str>) -> ValidationFlow {
        ValidationFlow {
            definition_source: definition_source.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn flow_belongs_to_action_matches_action_definition_source() {
        assert!(flow_belongs_to_action(
            &flow(Some("action.send-email")),
            "send-email"
        ));
        assert!(!flow_belongs_to_action(
            &flow(Some("action.send-email")),
            "other-action"
        ));
        assert!(!flow_belongs_to_action(&flow(None), "send-email"));
    }

    #[test]
    fn to_action_flow_projects_matching_fields() {
        let flow = ValidationFlow {
            flow_id: 42,
            project_id: 7,
            project_slug: "demo".to_string(),
            name: "My Flow".to_string(),
            r#type: "REST".to_string(),
            signature: "sig".to_string(),
            definition_source: Some("action.send-email".to_string()),
            ..Default::default()
        };

        let action_flow = to_action_flow(&flow);

        assert_eq!(action_flow.flow_id, 42);
        assert_eq!(action_flow.project_id, 7);
        assert_eq!(action_flow.project_slug, "demo");
        assert_eq!(action_flow.name, "My Flow");
    }
}
