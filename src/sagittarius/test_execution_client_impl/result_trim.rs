//! Trims an [`ExecutionResult`] down to fit the Sagittarius execution
//! stream's gRPC message-size limit.
//!
//! A single flow node can produce an arbitrarily large value (reading a big
//! file, returning a huge list, ...). Encoded into the stream message that
//! value can push the whole `ExecutionResult` past gRPC's per-message ceiling,
//! which Sagittarius' decoder rejects - tearing down the entire stream and
//! dropping every in-flight result with it.
//!
//! Rather than fail the execution, we keep the result sendable by cutting the
//! oversized value(s): the flow still reports `Success` overall, and only the
//! offending node's own output is replaced - a node result becomes a
//! `PayloadTooLarge` [`Error`] marker, while a cut parameter/input/top-level
//! value becomes a short string placeholder (no node-level error exists for
//! those positions). We repeatedly cut the single largest contributor until
//! the message fits, so a result with several large values still ends up
//! sendable.

use prost::Message;
use tucana::shared::{
    Error, ExecutionResult, Value, execution_result, node_execution_result, value::Kind,
};

/// Placeholder substituted for a plain `Value` that had to be cut (node
/// parameters, `input`, or a top-level success payload).
const OMITTED_VALUE_MARKER: &str = "<omitted: value exceeded size limit>";

/// Error `code`/`category`/`message` used when a *node's* result value is cut.
const PAYLOAD_TOO_LARGE_CODE: &str = "A-PAYLOAD-000001";
const PAYLOAD_TOO_LARGE_CATEGORY: &str = "PayloadTooLarge";
const PAYLOAD_TOO_LARGE_MESSAGE: &str = "value too large to display";

/// Values smaller than this aren't worth cutting: the gain wouldn't justify
/// destroying them, and it keeps already-cut markers from being reconsidered
/// (so the trim loop always makes progress instead of re-cutting placeholders).
const MIN_CUTTABLE_SIZE: usize = 512;

/// A single location inside an [`ExecutionResult`] whose value can be cut,
/// paired (via [`largest_target`]) with how many encoded bytes cutting it
/// would reclaim.
enum CutTarget {
    /// `node_execution_results[.0].result` is `Success` - replace with the
    /// `PayloadTooLarge` error marker.
    NodeResult(usize),
    /// `node_execution_results[.0].result` is `Error` - drop its `details`.
    NodeErrorDetails(usize),
    /// `node_execution_results[.0].parameter_results[.1].value` - replace with
    /// the string marker.
    NodeParameter(usize, usize),
    /// Top-level `result` is `Success` - replace its value with the string
    /// marker (the flow outcome itself stays `Success`).
    TopLevelResult,
    /// Top-level `result` is `Error` - drop its `details`.
    TopLevelErrorDetails,
    /// `input` - replace with the string marker.
    Input,
}

fn marker_value() -> Value {
    Value {
        kind: Some(Kind::StringValue(OMITTED_VALUE_MARKER.to_string())),
    }
}

fn payload_too_large_error() -> Error {
    Error {
        code: PAYLOAD_TOO_LARGE_CODE.to_string(),
        category: PAYLOAD_TOO_LARGE_CATEGORY.to_string(),
        message: PAYLOAD_TOO_LARGE_MESSAGE.to_string(),
        timestamp: crate::validation::epoch_millis_now(),
        version: crate::version::runtime_version().to_string(),
        ..Default::default()
    }
}

/// Finds the cut that would reclaim the most bytes, ignoring anything already
/// below [`MIN_CUTTABLE_SIZE`]. Returns `None` when nothing worth cutting
/// remains (everything left is already small, or already a marker).
fn largest_target(result: &ExecutionResult) -> Option<CutTarget> {
    let mut best: Option<(usize, CutTarget)> = None;

    let mut consider = |weight: usize, target: CutTarget| {
        if weight < MIN_CUTTABLE_SIZE {
            return;
        }
        if best.as_ref().is_none_or(|(best_weight, _)| weight > *best_weight) {
            best = Some((weight, target));
        }
    };

    if let Some(input) = result.input.as_ref() {
        consider(input.encoded_len(), CutTarget::Input);
    }

    match result.result.as_ref() {
        Some(execution_result::Result::Success(value)) => {
            consider(value.encoded_len(), CutTarget::TopLevelResult);
        }
        Some(execution_result::Result::Error(error)) => {
            if let Some(details) = error.details.as_ref() {
                consider(details.encoded_len(), CutTarget::TopLevelErrorDetails);
            }
        }
        None => {}
    }

    for (node_index, node) in result.node_execution_results.iter().enumerate() {
        match node.result.as_ref() {
            Some(node_execution_result::Result::Success(value)) => {
                consider(value.encoded_len(), CutTarget::NodeResult(node_index));
            }
            Some(node_execution_result::Result::Error(error)) => {
                if let Some(details) = error.details.as_ref() {
                    consider(
                        details.encoded_len(),
                        CutTarget::NodeErrorDetails(node_index),
                    );
                }
            }
            None => {}
        }

        for (param_index, parameter) in node.parameter_results.iter().enumerate() {
            if let Some(value) = parameter.value.as_ref() {
                consider(
                    value.encoded_len(),
                    CutTarget::NodeParameter(node_index, param_index),
                );
            }
        }
    }

    best.map(|(_, target)| target)
}

/// Applies one cut. Returns the 1-based node index whose value was cut, if the
/// cut was node-scoped, purely so the caller can log where it happened.
fn apply_cut(result: &mut ExecutionResult, target: CutTarget) -> Option<usize> {
    match target {
        CutTarget::Input => {
            result.input = Some(marker_value());
            None
        }
        CutTarget::TopLevelResult => {
            result.result = Some(execution_result::Result::Success(marker_value()));
            None
        }
        CutTarget::TopLevelErrorDetails => {
            if let Some(execution_result::Result::Error(error)) = result.result.as_mut() {
                error.details = None;
            }
            None
        }
        CutTarget::NodeResult(node_index) => {
            if let Some(node) = result.node_execution_results.get_mut(node_index) {
                node.result = Some(node_execution_result::Result::Error(
                    payload_too_large_error(),
                ));
            }
            Some(node_index)
        }
        CutTarget::NodeErrorDetails(node_index) => {
            if let Some(node) = result.node_execution_results.get_mut(node_index)
                && let Some(node_execution_result::Result::Error(error)) = node.result.as_mut()
            {
                error.details = None;
            }
            Some(node_index)
        }
        CutTarget::NodeParameter(node_index, param_index) => {
            if let Some(node) = result.node_execution_results.get_mut(node_index)
                && let Some(parameter) = node.parameter_results.get_mut(param_index)
            {
                parameter.value = Some(marker_value());
            }
            Some(node_index)
        }
    }
}

/// Trims `result` in place until its encoded size is at most `limit` bytes,
/// cutting the largest contributing value each pass. Returns the number of
/// cuts made (0 when it already fit).
///
/// If cutting every oversized value still isn't enough (e.g. a huge *number*
/// of individually-small node results), the node results are dropped wholesale
/// as a last resort so an oversized frame is never sent.
pub fn trim_execution_result_to_fit(result: &mut ExecutionResult, limit: usize) -> usize {
    let mut cuts = 0;

    while result.encoded_len() > limit {
        match largest_target(result) {
            Some(target) => {
                let node = apply_cut(result, target);
                cuts += 1;
                log::warn!(
                    "Cut oversized value from execution result execution_id={} node_index={} size_after_cut={} limit={}",
                    result.execution_identifier,
                    node.map(|index| index.to_string())
                        .unwrap_or_else(|| "n/a".to_string()),
                    result.encoded_len(),
                    limit
                );
            }
            None => {
                // Nothing individually large enough remains. If per-node
                // overhead is still pushing us over, drop the node results
                // entirely; otherwise we genuinely can't shrink further.
                if result.node_execution_results.is_empty() {
                    log::error!(
                        "Execution result still exceeds limit after trimming everything cuttable execution_id={} size={} limit={}",
                        result.execution_identifier,
                        result.encoded_len(),
                        limit
                    );
                    break;
                }
                result.node_execution_results.clear();
                cuts += 1;
                log::warn!(
                    "Dropped all node execution results to fit size limit execution_id={} size_after_cut={} limit={}",
                    result.execution_identifier,
                    result.encoded_len(),
                    limit
                );
            }
        }
    }

    cuts
}

#[cfg(test)]
mod tests {
    use super::*;
    use tucana::shared::{NodeExecutionResult, NodeParameterNodeExecutionResult};

    fn big_string_value(len: usize) -> Value {
        Value {
            kind: Some(Kind::StringValue("x".repeat(len))),
        }
    }

    fn node_with_result(result: node_execution_result::Result) -> NodeExecutionResult {
        NodeExecutionResult {
            started_at: 0,
            finished_at: 0,
            parameter_results: Vec::new(),
            id: Some(node_execution_result::Id::NodeId(1)),
            result: Some(result),
        }
    }

    fn base_result() -> ExecutionResult {
        ExecutionResult {
            execution_identifier: "exec-1".to_string(),
            flow_id: 42,
            started_at: 0,
            finished_at: 0,
            input: None,
            node_execution_results: Vec::new(),
            result: Some(execution_result::Result::Success(Value {
                kind: Some(Kind::BoolValue(true)),
            })),
        }
    }

    #[test]
    fn under_limit_result_is_untouched() {
        let mut result = base_result();
        let before = result.clone();

        let cuts = trim_execution_result_to_fit(&mut result, 1024);

        assert_eq!(cuts, 0);
        assert_eq!(result, before);
    }

    #[test]
    fn oversized_node_result_becomes_error_marker() {
        let mut result = base_result();
        result
            .node_execution_results
            .push(node_with_result(node_execution_result::Result::Success(
                big_string_value(10_000),
            )));
        // A second, small node that must stay untouched.
        result
            .node_execution_results
            .push(node_with_result(node_execution_result::Result::Success(
                Value {
                    kind: Some(Kind::BoolValue(true)),
                },
            )));

        let cuts = trim_execution_result_to_fit(&mut result, 2048);

        assert_eq!(cuts, 1);
        assert!(result.encoded_len() <= 2048);

        match &result.node_execution_results[0].result {
            Some(node_execution_result::Result::Error(error)) => {
                assert_eq!(error.code, PAYLOAD_TOO_LARGE_CODE);
                assert_eq!(error.category, PAYLOAD_TOO_LARGE_CATEGORY);
                assert_eq!(error.message, PAYLOAD_TOO_LARGE_MESSAGE);
            }
            other => panic!("expected cut node to be an error marker, got {other:?}"),
        }

        // Untouched node and the top-level success outcome are preserved.
        assert!(matches!(
            result.node_execution_results[1].result,
            Some(node_execution_result::Result::Success(Value {
                kind: Some(Kind::BoolValue(true))
            }))
        ));
        assert!(matches!(
            result.result,
            Some(execution_result::Result::Success(_))
        ));
    }

    #[test]
    fn oversized_parameter_value_becomes_string_marker() {
        let mut result = base_result();
        let mut node = node_with_result(node_execution_result::Result::Success(Value {
            kind: Some(Kind::BoolValue(true)),
        }));
        node.parameter_results.push(NodeParameterNodeExecutionResult {
            value: Some(big_string_value(10_000)),
        });
        result.node_execution_results.push(node);

        let cuts = trim_execution_result_to_fit(&mut result, 2048);

        assert_eq!(cuts, 1);
        assert!(result.encoded_len() <= 2048);
        assert_eq!(
            result.node_execution_results[0].parameter_results[0].value,
            Some(marker_value())
        );
    }

    #[test]
    fn oversized_top_level_result_stays_success_with_marker() {
        let mut result = base_result();
        result.result = Some(execution_result::Result::Success(big_string_value(10_000)));

        let cuts = trim_execution_result_to_fit(&mut result, 2048);

        assert_eq!(cuts, 1);
        assert!(result.encoded_len() <= 2048);
        assert!(matches!(
            result.result,
            Some(execution_result::Result::Success(Value {
                kind: Some(Kind::StringValue(_))
            }))
        ));
    }

    #[test]
    fn oversized_input_becomes_string_marker() {
        let mut result = base_result();
        result.input = Some(big_string_value(10_000));

        let cuts = trim_execution_result_to_fit(&mut result, 2048);

        assert_eq!(cuts, 1);
        assert!(result.encoded_len() <= 2048);
        assert_eq!(result.input, Some(marker_value()));
    }

    #[test]
    fn many_small_nodes_are_dropped_as_last_resort() {
        let mut result = base_result();
        for _ in 0..5_000 {
            result
                .node_execution_results
                .push(node_with_result(node_execution_result::Result::Success(
                    big_string_value(100),
                )));
        }
        assert!(result.encoded_len() > 2048);

        let cuts = trim_execution_result_to_fit(&mut result, 2048);

        assert!(cuts >= 1);
        assert!(result.encoded_len() <= 2048);
        assert!(result.node_execution_results.is_empty());
    }
}
