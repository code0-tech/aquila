use tucana::shared::ValidationFlow;

/// Every flow identifier has this key
/// <type>.<project_slug>.<flow_id>
pub fn get_flow_identifier(flow: &ValidationFlow) -> String {
    format!("{}.{}.{}.{}", flow.r#type, flow.project_slug, flow.project_id, flow.flow_id)
}
