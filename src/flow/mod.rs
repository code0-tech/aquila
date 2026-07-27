//! The key scheme Aquila's flow KV store uses, and the two operations built
//! on it: computing a flow's key and checking whether a key belongs to a
//! given flow id. There is no secondary index, so every "find by flow id"
//! lookup elsewhere in the codebase is a scan using [`key_has_flow_id`].

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

#[cfg(test)]
mod tests {
    use super::key_has_flow_id;

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
}
