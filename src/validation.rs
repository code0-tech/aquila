//! Validates incoming REST-flow execution requests against the JSON Schema
//! stored in the flow's `input_schema` setting.
//!
//! Mirrors draco's REST adapter (`adapter/rest/src/validation.rs` and
//! `route::extract_flow_setting_as_struct`), but here the outcome of a failed
//! validation isn't an HTTP response - it's a synthesized [`ExecutionResult`]
//! that gets sent straight back to Sagittarius instead of the flow ever being
//! dispatched to a runtime.

use lupus::data::{Data, Number};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tucana::shared::{
    Error, ExecutionResult, Struct, ValidationFlow, Value, execution_result,
    helper::value::to_json_value, value::Kind,
};

const REST_FLOW_TYPE: &str = "REST";
const INPUT_SCHEMA_SETTING_ID: &str = "input_schema";

/// Whether `flow` is a REST (webhook) flow - the only flow type that carries
/// a request body to validate against `input_schema` before dispatch.
pub fn is_rest_flow(flow: &ValidationFlow) -> bool {
    flow.r#type == REST_FLOW_TYPE
}

/// Reads the `input_schema` flow setting, the same way draco's REST adapter
/// reads `httpMethod`/`httpURL`/`input_schema` off `flow.settings`.
pub fn extract_input_schema(flow: &ValidationFlow) -> Option<&Struct> {
    let setting = match flow
        .settings
        .iter()
        .find(|setting| setting.flow_setting_id == INPUT_SCHEMA_SETTING_ID)
    {
        Some(setting) => setting,
        None => {
            log::debug!(
                "flow setting is missing: flow_id={} flow_setting_id={}",
                flow.flow_id,
                INPUT_SCHEMA_SETTING_ID
            );
            return None;
        }
    };

    let value = match setting.value.as_ref() {
        Some(value) => value,
        None => {
            log::debug!(
                "flow setting has no value: flow_id={} flow_setting_id={}",
                flow.flow_id,
                INPUT_SCHEMA_SETTING_ID
            );
            return None;
        }
    };

    let kind = match value.kind.as_ref() {
        Some(kind) => kind,
        None => {
            log::debug!(
                "flow setting has no kind: flow_id={} flow_setting_id={}",
                flow.flow_id,
                INPUT_SCHEMA_SETTING_ID
            );
            return None;
        }
    };

    match kind {
        Kind::StructValue(value) => Some(value),
        _ => {
            log::debug!(
                "flow setting has non-struct kind: flow_id={} flow_setting_id={} kind={:?}",
                flow.flow_id,
                INPUT_SCHEMA_SETTING_ID,
                kind
            );
            None
        }
    }
}

#[derive(Debug)]
pub enum BodyValidationError {
    InvalidSchema(String),
    InvalidBody(String),
    Validation(String),
}

impl std::fmt::Display for BodyValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSchema(msg) => write!(f, "flow input schema is invalid: {}", msg),
            Self::InvalidBody(msg) => write!(f, "request body could not be validated: {}", msg),
            Self::Validation(msg) => write!(f, "request body failed schema validation: {}", msg),
        }
    }
}

impl std::error::Error for BodyValidationError {}

/// Validates `body` against `input_schema` (a JSON Schema stored as a `shared.Struct` on the
/// flow). A flow without an `input_schema` (or with an empty one) accepts any body unvalidated.
pub fn validate_body_against_schema(
    input_schema: Option<&Struct>,
    body: Option<&Value>,
) -> Result<(), BodyValidationError> {
    let Some(input_schema) = input_schema.filter(|schema| !schema.fields.is_empty()) else {
        return Ok(());
    };

    let schema_json = to_json_value(Value {
        kind: Some(Kind::StructValue(input_schema.clone())),
    });
    let raw = serde_json::to_string(&schema_json)
        .map_err(|err| BodyValidationError::InvalidSchema(err.to_string()))?;
    let schema = lupus::JsonSchema { raw };

    let body_value = body.cloned().unwrap_or(Value {
        kind: Some(Kind::NullValue(0)),
    });
    let body_json = to_json_value(body_value);
    let data = json_value_to_data(body_json)?;

    lupus::validation::validate_json_schema(&data, &schema)
        .map_err(|err| BodyValidationError::Validation(err.to_string()))
}

/// Mirrors lupus's internal JSON-to-`Data` conversion. We can't reuse `lupus::formats::json`
/// directly here because the intermediate `tucana::shared::Value` type in this workspace and the
/// one `lupus` depends on resolve to different (semver-incompatible 0.0.x) versions of the
/// `tucana` crate, so we go through `serde_json::Value` instead, which both sides share.
fn json_value_to_data(value: serde_json::Value) -> Result<Data, BodyValidationError> {
    match value {
        serde_json::Value::Null => Ok(Data::Null),
        serde_json::Value::Bool(value) => Ok(Data::Bool(value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Data::Number(Number::I64(value)))
            } else if let Some(value) = value.as_u64() {
                Ok(Data::Number(Number::U64(value)))
            } else if let Some(value) = value.as_f64() {
                Ok(Data::Number(Number::F64(value)))
            } else {
                Err(BodyValidationError::InvalidBody(
                    "unsupported JSON number".to_string(),
                ))
            }
        }
        serde_json::Value::String(value) => Ok(Data::String(value)),
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(json_value_to_data)
            .collect::<Result<Vec<_>, _>>()
            .map(Data::Array),
        serde_json::Value::Object(fields) => fields
            .into_iter()
            .map(|(key, value)| Ok((key, json_value_to_data(value)?)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(Data::Object),
    }
}

/// Builds the `ExecutionResult` sent straight back to Sagittarius when a REST flow's body fails
/// `input_schema` validation, so the flow is never dispatched to a runtime for execution.
pub fn rejection_result(
    execution_identifier: String,
    flow_id: i64,
    error: &BodyValidationError,
) -> ExecutionResult {
    let now = epoch_millis_now();
    ExecutionResult {
        execution_identifier,
        flow_id,
        started_at: now,
        finished_at: now,
        result: Some(execution_result::Result::Error(Error {
            code: "A-VALIDATION-000001".to_string(),
            category: "InvalidArgument".to_string(),
            message: error.to_string(),
            timestamp: now,
            version: crate::version::runtime_version().to_string(),
            ..Default::default()
        })),
        ..Default::default()
    }
}

fn epoch_millis_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(error) => {
            log::warn!("System time before UNIX_EPOCH: {:?}", error);
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tucana::shared::FlowSetting;

    fn schema_struct(raw_schema: serde_json::Value) -> Struct {
        let Value {
            kind: Some(Kind::StructValue(schema)),
        } = tucana::shared::helper::value::from_json_value(raw_schema)
        else {
            panic!("expected object schema");
        };
        schema
    }

    #[test]
    fn rest_flow_type_is_detected() {
        let flow = ValidationFlow {
            r#type: "REST".to_string(),
            ..Default::default()
        };
        assert!(is_rest_flow(&flow));

        let flow = ValidationFlow {
            r#type: "CRON".to_string(),
            ..Default::default()
        };
        assert!(!is_rest_flow(&flow));
    }

    #[test]
    fn missing_schema_allows_any_body() {
        let body = Value {
            kind: Some(Kind::StringValue("anything".to_string())),
        };
        assert!(validate_body_against_schema(None, Some(&body)).is_ok());
    }

    #[test]
    fn empty_schema_allows_any_body() {
        let schema = Struct {
            fields: HashMap::new(),
        };
        let body = Value {
            kind: Some(Kind::StringValue("anything".to_string())),
        };
        assert!(validate_body_against_schema(Some(&schema), Some(&body)).is_ok());
    }

    #[test]
    fn matching_body_passes_validation() {
        let schema = schema_struct(serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": { "type": "string" }
            }
        }));

        let body = tucana::shared::helper::value::from_json_value(serde_json::json!({
            "name": "Ada"
        }));

        assert!(validate_body_against_schema(Some(&schema), Some(&body)).is_ok());
    }

    #[test]
    fn mismatched_body_fails_validation() {
        let schema = schema_struct(serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": { "type": "string" }
            }
        }));

        let body = tucana::shared::helper::value::from_json_value(serde_json::json!({
            "age": 42
        }));

        let err = validate_body_against_schema(Some(&schema), Some(&body)).unwrap_err();
        assert!(matches!(err, BodyValidationError::Validation(_)));
    }

    #[test]
    fn extract_input_schema_reads_struct_value_setting() {
        let schema = schema_struct(serde_json::json!({ "type": "object" }));
        let flow = ValidationFlow {
            flow_id: 1,
            settings: vec![FlowSetting {
                database_id: None,
                flow_setting_id: "input_schema".to_string(),
                value: Some(Value {
                    kind: Some(Kind::StructValue(schema.clone())),
                }),
                cast: None,
            }],
            ..Default::default()
        };

        assert_eq!(extract_input_schema(&flow), Some(&schema));
    }

    #[test]
    fn extract_input_schema_is_none_when_setting_missing() {
        let flow = ValidationFlow::default();
        assert_eq!(extract_input_schema(&flow), None);
    }

    #[test]
    fn rejection_result_carries_error_message() {
        let error = BodyValidationError::Validation("at $/name: expected string".to_string());
        let result = rejection_result("exec-1".to_string(), 42, &error);

        assert_eq!(result.execution_identifier, "exec-1");
        assert_eq!(result.flow_id, 42);
        match result.result {
            Some(execution_result::Result::Error(err)) => {
                assert!(err.message.contains("expected string"));
            }
            other => panic!("expected error result, got {:?}", other),
        }
    }
}
