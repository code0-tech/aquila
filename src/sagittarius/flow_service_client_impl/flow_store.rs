//! KV-store operations backing the flows Aquila keeps synchronized with
//! Sagittarius: delete-by-id, wholesale replace, and single-flow upsert.
//! Every flow is keyed by [`get_flow_identifier`], so lookups by id require
//! a full key scan rather than a direct get.

use futures::{StreamExt, TryStreamExt};
use prost::Message;
use tucana::shared::{Flows, ValidationFlow};

use crate::flow::{get_flow_identifier, key_has_flow_id};

/// Deletes every stored key matching `flow_id`, returning how many were
/// removed and the `definition_source` of each one that had one - read
/// before deletion, since it's the only way for the caller to know which
/// action(s) a now-deleted flow belonged to.
pub(super) async fn delete_flow(
    store: &async_nats::jetstream::kv::Store,
    flow_id: i64,
) -> (usize, Vec<String>) {
    let mut keys = match store.keys().await {
        Ok(keys) => keys.boxed(),
        Err(err) => {
            log::error!(
                "Failed to list stored flows for deletion flow_id={} error={:?}",
                flow_id,
                err
            );
            return (0, Vec::new());
        }
    };

    let mut deleted_count = 0;
    let mut definition_sources = Vec::new();
    while let Ok(Some(key)) = keys.try_next().await {
        if !key_has_flow_id(&key, flow_id) {
            continue;
        }

        let definition_source = match store.get(&key).await {
            Ok(Some(bytes)) => ValidationFlow::decode(bytes)
                .ok()
                .and_then(|flow| flow.definition_source),
            Ok(None) => None,
            Err(err) => {
                log::warn!(
                    "Failed to read stored flow before deletion flow_id={} key={} error={:?}",
                    flow_id,
                    key,
                    err
                );
                None
            }
        };

        match store.delete(&key).await {
            Ok(_) => {
                deleted_count += 1;
                if let Some(source) = definition_source {
                    definition_sources.push(source);
                }
            }
            Err(err) => log::error!(
                "Failed to delete stored flow flow_id={} key={} error={:?}",
                flow_id,
                key,
                err
            ),
        }
    }

    (deleted_count, definition_sources)
}

/// Stores a single flow update under its computed key, returning that key
/// for the caller's own logging/metrics.
pub(super) async fn store_flow(
    store: &async_nats::jetstream::kv::Store,
    flow: &ValidationFlow,
) -> (String, Result<(), async_nats::jetstream::kv::PutError>) {
    let key = get_flow_identifier(flow);
    let bytes = flow.encode_to_vec();
    let result = store.put(key.clone(), bytes.into()).await.map(|_| ());
    (key, result)
}

/// Purges every currently stored flow and replaces it with `flows`.
/// Returns `(purged_count, stored_count)` so the caller can report metrics.
pub(super) async fn replace_all(
    store: &async_nats::jetstream::kv::Store,
    flows: Flows,
) -> (usize, usize) {
    let mut keys = match store.keys().await {
        Ok(keys) => keys.boxed(),
        Err(err) => {
            log::error!(
                "Failed to list stored flows before replacement error={:?}",
                err
            );
            return (0, 0);
        }
    };

    let mut purged_count = 0;
    while let Ok(Some(key)) = keys.try_next().await {
        match store.purge(&key).await {
            Ok(_) => purged_count += 1,
            Err(err) => log::error!("Failed to purge stored flow key={} error={}", key, err),
        }
    }

    let mut stored_count = 0;
    for flow in flows.flows {
        let (key, result) = store_flow(store, &flow).await;
        match result {
            Ok(()) => {
                stored_count += 1;
                log::debug!("Stored replacement flow key={}", key);
            }
            Err(err) => log::error!(
                "Failed to store replacement flow key={} error={:?}",
                key,
                err
            ),
        }
    }

    (purged_count, stored_count)
}
