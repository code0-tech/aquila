//! A bounded, TTL-expiring cache from execution id to flow id.
//!
//! Sagittarius execution results don't always carry a `flow_id` back (some
//! runtimes only echo the execution id), so Aquila remembers the mapping it
//! made when it first dispatched the execution and re-attaches the flow id
//! when the result comes back. The cache is bounded and TTL'd purely as a
//! safety net against executions that never send a result — nothing here
//! bounds it in the happy path, since [`ExecutionFlowIdCache::take`] removes
//! the entry as soon as it's used.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::Mutex;

/// How long a mapping is kept if its execution never sends a result.
const EXECUTION_FLOW_ID_TTL: Duration = Duration::from_secs(30 * 60);
/// Upper bound on cache size, enforced by evicting the entry closest to
/// expiry when a new one would push the cache over the limit.
const MAX_EXECUTION_FLOW_IDS: usize = 10_000;

struct ExecutionFlowIdMapping {
    flow_id: i64,
    expires_at: Instant,
}

#[derive(Clone, Default)]
pub(super) struct ExecutionFlowIdCache {
    entries: Arc<Mutex<HashMap<String, ExecutionFlowIdMapping>>>,
}

impl ExecutionFlowIdCache {
    /// Remembers `flow_id` under `execution_id` for [`EXECUTION_FLOW_ID_TTL`].
    /// A no-op for an empty `execution_id`, since that can never be looked
    /// back up by [`take`](Self::take).
    pub(super) async fn remember(&self, execution_id: &str, flow_id: i64) {
        if execution_id.is_empty() {
            log::warn!("Cannot remember execution flow_id because execution_id is empty");
            return;
        }

        let mut entries = self.entries.lock().await;
        let now = Instant::now();
        let expired_count = prune_expired(&mut entries, now);
        let replacing_existing = entries.contains_key(execution_id);
        let evicted_execution_id = if !replacing_existing && entries.len() >= MAX_EXECUTION_FLOW_IDS
        {
            remove_soonest_to_expire(&mut entries)
        } else {
            None
        };

        entries.insert(
            execution_id.to_string(),
            ExecutionFlowIdMapping {
                flow_id,
                expires_at: now + EXECUTION_FLOW_ID_TTL,
            },
        );

        if let Some(evicted_execution_id) = evicted_execution_id {
            log::warn!(
                "Evicted execution flow mapping because cache is full evicted_execution_id={} max_entries={}",
                evicted_execution_id,
                MAX_EXECUTION_FLOW_IDS
            );
        }
        log::debug!(
            "Remembered execution flow mapping execution_id={} flow_id={} cached_entries={} expired_entries={}",
            execution_id,
            flow_id,
            entries.len(),
            expired_count
        );
    }

    /// Drops the mapping for `execution_id` without returning it, used once
    /// a result has arrived carrying its own `flow_id` and the cached one is
    /// no longer needed.
    pub(super) async fn forget(&self, execution_id: &str) {
        if execution_id.is_empty() {
            return;
        }

        let mut entries = self.entries.lock().await;
        let removed = entries.remove(execution_id).is_some();
        log::debug!(
            "Forgot execution flow mapping execution_id={} removed={}",
            execution_id,
            removed
        );
    }

    /// Removes and returns the flow id for `execution_id`, unless it has
    /// already expired.
    pub(super) async fn take(&self, execution_id: &str) -> Option<i64> {
        if execution_id.is_empty() {
            return None;
        }

        let mut entries = self.entries.lock().await;
        let mapping = entries.remove(execution_id)?;
        if mapping.expires_at > Instant::now() {
            Some(mapping.flow_id)
        } else {
            log::debug!(
                "Dropped expired execution flow mapping execution_id={}",
                execution_id
            );
            None
        }
    }
}

fn prune_expired(entries: &mut HashMap<String, ExecutionFlowIdMapping>, now: Instant) -> usize {
    let initial_len = entries.len();
    entries.retain(|_, mapping| mapping.expires_at > now);
    initial_len - entries.len()
}

/// Evicts whichever entry is closest to expiring, since that's the best
/// approximation of "oldest" without tracking insertion order separately.
fn remove_soonest_to_expire(entries: &mut HashMap<String, ExecutionFlowIdMapping>) -> Option<String> {
    let soonest_execution_id = entries
        .iter()
        .min_by_key(|(_, mapping)| mapping.expires_at)
        .map(|(execution_id, _)| execution_id.clone())?;

    entries.remove(&soonest_execution_id);
    Some(soonest_execution_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_returns_and_removes_the_mapping() {
        futures::executor::block_on(async {
            let cache = ExecutionFlowIdCache::default();
            cache.remember("execution-1", 42).await;

            assert_eq!(cache.take("execution-1").await, Some(42));
            assert_eq!(cache.take("execution-1").await, None);
        });
    }

    #[test]
    fn forget_removes_without_returning() {
        futures::executor::block_on(async {
            let cache = ExecutionFlowIdCache::default();
            cache.remember("execution-1", 42).await;
            cache.forget("execution-1").await;

            assert_eq!(cache.take("execution-1").await, None);
        });
    }

    #[test]
    fn empty_execution_id_is_ignored() {
        futures::executor::block_on(async {
            let cache = ExecutionFlowIdCache::default();
            cache.remember("", 42).await;

            assert_eq!(cache.take("").await, None);
        });
    }

    #[test]
    fn take_drops_expired_mappings() {
        futures::executor::block_on(async {
            let cache = ExecutionFlowIdCache::default();
            cache.remember("execution-1", 42).await;

            {
                let mut entries = cache.entries.lock().await;
                entries.get_mut("execution-1").unwrap().expires_at =
                    Instant::now() - Duration::from_secs(1);
            }

            assert_eq!(cache.take("execution-1").await, None);
        });
    }
}
