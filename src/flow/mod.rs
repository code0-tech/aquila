//! The key scheme Aquila's flow KV store uses, and the two operations built
//! on it: computing a flow's key and checking whether a key belongs to a
//! given flow id. There is no secondary index, so every "find by flow id"
//! lookup elsewhere in the codebase is a scan using [`key_has_flow_id`].

use tucana::aquila::ActionFlow;
use tucana::shared::ValidationFlow;

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
    }
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
