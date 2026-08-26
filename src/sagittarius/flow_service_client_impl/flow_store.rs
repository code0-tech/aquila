//! KV-store operations backing the flows Aquila keeps synchronized with
//! Sagittarius: delete-by-id, wholesale replace, and single-flow upsert.
//! Every flow is keyed directly by its globally unique flow id.

use futures::{StreamExt, TryStreamExt};
use prost::Message;
use tucana::shared::{Flows, ValidationFlow};

use crate::flow::get_flow_identifier;

/// Deletes the flow stored under `flow_id`, returning whether it existed and
/// its `definition_source` when present. The source is read before deletion
/// so the caller can notify the action that owned the deleted flow.
pub(super) async fn delete_flow(
    store: &async_nats::jetstream::kv::Store,
    flow_id: i64,
) -> (usize, Vec<String>) {
    let key = flow_id.to_string();
    let definition_source = match store.get(&key).await {
        Ok(Some(bytes)) => ValidationFlow::decode(bytes)
            .ok()
            .and_then(|flow| flow.definition_source),
        Ok(None) => return (0, Vec::new()),
        Err(err) => {
            log::error!(
                "Failed to read stored flow before deletion flow_id={} key={} error={:?}",
                flow_id,
                key,
                err
            );
            return (0, Vec::new());
        }
    };

    match store.delete(&key).await {
        Ok(()) => (1, definition_source.into_iter().collect()),
        Err(err) => {
            log::error!(
                "Failed to delete stored flow flow_id={} key={} error={:?}",
                flow_id,
                key,
                err
            );
            (0, Vec::new())
        }
    }
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
