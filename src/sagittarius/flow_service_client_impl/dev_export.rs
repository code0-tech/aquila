//! Development-only convenience export of the current flow set to a local
//! JSON file, so a developer can inspect what Sagittarius last synced
//! without needing NATS/KV tooling. Only called when running with
//! `environment == DEVELOPMENT`.
//!
//! The destination is `config.static_config.flow_path` — the same path
//! static mode loads from at startup — so a development export can be fed
//! straight back in as a static-mode fallback.

use std::path::Path;

use tokio::fs;
use tucana::shared::{Flows, ValidationFlow};

/// Reads the current export at `path`, treating a missing or unparsable
/// file as an empty set so callers can always fall back to a fresh export.
async fn read_flows(path: &str) -> Flows {
    match fs::read(path).await {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(flows) => flows,
            Err(error) => {
                log::warn!(
                    "Failed to parse existing development flow export path={} error={:?}; starting from an empty export",
                    path,
                    error
                );
                Flows::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Flows::default(),
        Err(error) => {
            log::warn!(
                "Failed to read existing development flow export path={} error={}; starting from an empty export",
                path,
                error
            );
            Flows::default()
        }
    }
}

/// Writes `flows` to `path`, replacing any previous export.
///
/// Goes through a `path`-adjacent temp file plus rename so a reader never
/// observes a partially written export. If the rename fails because the
/// destination already exists, the destination is removed and the rename
/// retried once before giving up. Returns whether the write succeeded.
async fn write(path: &str, flows: &Flows) -> bool {
    let json = match serde_json::to_vec_pretty(flows) {
        Ok(bytes) => bytes,
        Err(error) => {
            log::error!(
                "Failed to serialize development flow export flow_count={} error={:?}",
                flows.flows.len(),
                error
            );
            return false;
        }
    };

    let final_path = Path::new(path);
    let tmp_path_buf = format!("{path}.tmp");
    let tmp_path = Path::new(&tmp_path_buf);

    if let Err(error) = fs::write(tmp_path, &json).await {
        log::error!(
            "Failed to write development flow export path={} error={}",
            tmp_path.display(),
            error
        );
        return false;
    }

    if let Err(error) = fs::rename(tmp_path, final_path).await {
        log::warn!(
            "Could not atomically replace development flow export path={} error={}; retrying after removing destination",
            final_path.display(),
            error
        );
        match fs::remove_file(final_path).await {
            Ok(()) => log::debug!(
                "Removed previous development flow export path={}",
                final_path.display()
            ),
            Err(remove_error) if remove_error.kind() == std::io::ErrorKind::NotFound => {}
            Err(remove_error) => log::warn!(
                "Failed to remove previous development flow export path={} error={}",
                final_path.display(),
                remove_error
            ),
        }
        if let Err(retry_error) = fs::rename(tmp_path, final_path).await {
            log::error!(
                "Failed to replace development flow export source_path={} destination_path={} initial_rename_error={} retry_error={}",
                tmp_path.display(),
                final_path.display(),
                error,
                retry_error
            );
            if let Err(cleanup_error) = fs::remove_file(tmp_path).await {
                log::warn!(
                    "Failed to clean up temporary development flow export path={} error={}",
                    tmp_path.display(),
                    cleanup_error
                );
            }
            return false;
        }
    }

    true
}

/// Overwrites `path` with `flows` wholesale.
pub(super) async fn overwrite(path: &str, flows: Flows) {
    let flow_count = flows.flows.len();
    if write(path, &flows).await {
        log::info!("Exported {} flows to {}", flow_count, path);
    }
}

/// Adds or replaces a single flow in the export at `path`, read-modify-write.
pub(super) async fn upsert_flow(path: &str, flow: ValidationFlow) {
    let mut flows = read_flows(path).await;
    let flow_id = flow.flow_id;

    match flows.flows.iter_mut().find(|f| f.flow_id == flow_id) {
        Some(existing) => *existing = flow,
        None => flows.flows.push(flow),
    }

    if write(path, &flows).await {
        log::info!(
            "Upserted flow_id={} into development flow export path={}",
            flow_id,
            path
        );
    }
}

/// Removes a single flow from the export at `path` by id, read-modify-write.
/// Logs a warning (without failing) if no matching flow was found.
pub(super) async fn remove_flow(path: &str, flow_id: i64) {
    let mut flows = read_flows(path).await;
    let original_count = flows.flows.len();
    flows.flows.retain(|f| f.flow_id != flow_id);

    if flows.flows.len() == original_count {
        log::warn!(
            "Development flow export had no flow matching flow_id={} to remove path={}",
            flow_id,
            path
        );
        return;
    }

    if write(path, &flows).await {
        log::info!(
            "Removed flow_id={} from development flow export path={}",
            flow_id,
            path
        );
    }
}
