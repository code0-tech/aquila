//! Development-only convenience export of the current flow set to a local
//! JSON file, so a developer can inspect what Sagittarius last synced
//! without needing NATS/KV tooling. Only called when running with
//! `environment == DEVELOPMENT`.

use std::path::Path;

use tokio::fs;
use tucana::shared::Flows;

const EXPORT_PATH: &str = "flowExport.json";
const EXPORT_TMP_PATH: &str = "flowExport.json.tmp";

/// Writes `flows` to `flowExport.json`, replacing any previous export.
///
/// Goes through a temp file plus rename so a reader never observes a
/// partially written export. If the rename fails because the destination
/// already exists, the destination is removed and the rename retried once
/// before giving up.
pub(super) async fn overwrite(flows: Flows) {
    let json = match serde_json::to_vec_pretty(&flows) {
        Ok(bytes) => bytes,
        Err(error) => {
            log::error!(
                "Failed to serialize development flow export flow_count={} error={:?}",
                flows.flows.len(),
                error
            );
            return;
        }
    };

    let final_path = Path::new(EXPORT_PATH);
    let tmp_path = Path::new(EXPORT_TMP_PATH);

    if let Err(error) = fs::write(tmp_path, &json).await {
        log::error!(
            "Failed to write development flow export path={} error={}",
            tmp_path.display(),
            error
        );
        return;
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
            return;
        }
    }

    log::info!(
        "Exported {} flows to {}",
        flows.flows.len(),
        final_path.display()
    );
}
