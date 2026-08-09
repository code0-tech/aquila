//! Tracks which shard indices are claimed for each action identifier
//! connected in `Split` scaling mode, so concurrent connections for the same
//! identifier partition project-scoped updates between them by `project_id`
//! instead of every connection receiving everything.
//!
//! Assignment is static for the lifetime of a connection: an index, once
//! claimed, is never taken away from a live connection to hand to another -
//! there's no protocol message to tell an already-connected action "you no
//! longer own project X". If fewer connections claim shards than the
//! configured replica count, the unclaimed indices' projects simply get no
//! updates until another connection claims them; see [`ActionShardRegistry::unclaimed`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::Mutex;

/// A connection's place in an action identifier's shard space: it owns every
/// `project_id` that hashes to `index` out of `replicas` total shards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShardAssignment {
    pub index: u32,
    pub replicas: u32,
}

impl ShardAssignment {
    pub fn owns(&self, project_id: i64) -> bool {
        shard_of(project_id, self.replicas) == self.index
    }
}

/// Which shard index `project_id` belongs to out of `replicas` total shards.
pub fn shard_of(project_id: i64, replicas: u32) -> u32 {
    project_id.rem_euclid(replicas.max(1) as i64) as u32
}

#[derive(Clone, Default)]
pub struct ActionShardRegistry {
    inner: Arc<Mutex<HashMap<String, HashSet<u32>>>>,
}

impl ActionShardRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Claims the lowest free shard index in `0..replicas` for `identifier`.
    /// Returns `None` if every index is already claimed by another
    /// connection - i.e. more connections have logged on than `replicas` allows.
    pub async fn claim(&self, identifier: &str, replicas: u32) -> Option<u32> {
        let mut guard = self.inner.lock().await;
        let claimed = guard.entry(identifier.to_string()).or_default();
        let index = (0..replicas).find(|i| !claimed.contains(i))?;
        claimed.insert(index);
        Some(index)
    }

    /// Releases a previously claimed shard index, freeing it for the next
    /// connection to claim.
    pub async fn release(&self, identifier: &str, index: u32) {
        let mut guard = self.inner.lock().await;
        if let Some(claimed) = guard.get_mut(identifier) {
            claimed.remove(&index);
            if claimed.is_empty() {
                guard.remove(identifier);
            }
        }
    }

    /// Every shard index in `0..replicas` with no current claimant, so
    /// callers can surface a configured-but-unclaimed shard instead of
    /// letting its projects silently stop receiving updates.
    pub async fn unclaimed(&self, identifier: &str, replicas: u32) -> Vec<u32> {
        let guard = self.inner.lock().await;
        let claimed = guard.get(identifier);
        (0..replicas)
            .filter(|index| !claimed.is_some_and(|c| c.contains(index)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_of_distributes_sequential_project_ids_round_robin() {
        assert_eq!(shard_of(1, 2), 1);
        assert_eq!(shard_of(2, 2), 0);
        assert_eq!(shard_of(3, 2), 1);
        assert_eq!(shard_of(4, 2), 0);
    }

    #[test]
    fn shard_of_is_stable_with_a_single_replica() {
        assert_eq!(shard_of(1, 1), 0);
        assert_eq!(shard_of(42, 1), 0);
    }

    #[test]
    fn shard_assignment_owns_matches_only_its_own_shard() {
        let assignment = ShardAssignment {
            index: 1,
            replicas: 2,
        };

        assert!(assignment.owns(1));
        assert!(!assignment.owns(2));
    }

    #[tokio::test]
    async fn claim_hands_out_the_lowest_free_index_and_rejects_when_exhausted() {
        let registry = ActionShardRegistry::new();

        assert_eq!(registry.claim("action", 2).await, Some(0));
        assert_eq!(registry.claim("action", 2).await, Some(1));
        assert_eq!(registry.claim("action", 2).await, None);
    }

    #[tokio::test]
    async fn release_frees_the_index_for_reclaiming() {
        let registry = ActionShardRegistry::new();

        registry.claim("action", 1).await;
        registry.release("action", 0).await;

        assert_eq!(registry.claim("action", 1).await, Some(0));
    }

    #[tokio::test]
    async fn unclaimed_reports_gaps_in_the_shard_space() {
        let registry = ActionShardRegistry::new();

        assert_eq!(registry.unclaimed("action", 2).await, vec![0, 1]);

        registry.claim("action", 2).await;
        assert_eq!(registry.unclaimed("action", 2).await, vec![1]);
    }

    #[tokio::test]
    async fn claims_for_different_identifiers_are_independent() {
        let registry = ActionShardRegistry::new();

        assert_eq!(registry.claim("a", 1).await, Some(0));
        assert_eq!(registry.claim("b", 1).await, Some(0));
    }

    /// A project must never be served by two connections at once, and never
    /// by the "wrong" one while the connection that actually owns its shard
    /// index is still connected - the index a project maps to never changes,
    /// and only one connection can hold that index at a time.
    #[tokio::test]
    async fn a_project_is_never_owned_by_more_than_one_live_connection() {
        let registry = ActionShardRegistry::new();
        let replicas = 2;
        let project_five = 5i64;

        let first_index = registry.claim("action", replicas).await.unwrap();
        let second_index = registry.claim("action", replicas).await.unwrap();
        assert_ne!(first_index, second_index);

        let first = ShardAssignment {
            index: first_index,
            replicas,
        };
        let second = ShardAssignment {
            index: second_index,
            replicas,
        };

        // Exactly one of the two live connections owns project 5, never both.
        assert_ne!(first.owns(project_five), second.owns(project_five));

        // And that ownership is stable: reclaiming (e.g. a third connection
        // trying to join) can't hand project 5's index to anyone else while
        // its owner is still connected.
        assert_eq!(registry.claim("action", replicas).await, None);
    }
}
