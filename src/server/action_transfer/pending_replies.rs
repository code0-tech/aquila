//! Tracks in-flight action executions so their NATS reply subject can be
//! located again once the connected action sends back a result.

use async_nats::Subject;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::Mutex;

/// A single execution awaiting a result, plus everything needed to route
/// that result and measure how long it took.
#[derive(Clone)]
pub(super) struct PendingReply {
    /// Where to publish the `ActionExecutionResponse` once it arrives.
    pub(super) reply_subject: Subject,
    /// Every key this entry is filed under in the store, so removal by any
    /// one alias can also clean up the others.
    keys: Vec<String>,
    /// When the request was forwarded to the action, for execution-duration metrics.
    pub(super) started_at: Instant,
}

/// Shared, lock-protected registry mapping execution identifiers to the NATS
/// reply subject a result must eventually be published to.
#[derive(Clone, Default)]
pub(super) struct PendingReplyStore {
    inner: Arc<Mutex<HashMap<String, PendingReply>>>,
}

impl PendingReplyStore {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Registers `reply_subject` under every alias in `keys` (typically the
    /// payload execution id and the one derived from the NATS subject).
    pub(super) async fn insert(&self, reply_subject: Subject, keys: Vec<String>) {
        let pending_reply = PendingReply {
            reply_subject,
            keys: keys.clone(),
            started_at: Instant::now(),
        };

        let mut pending = self.inner.lock().await;
        for key in keys {
            pending.insert(key, pending_reply.clone());
        }
    }

    /// Removes and returns the reply registered under `execution_id`, along
    /// with any of its aliases.
    pub(super) async fn remove(&self, execution_id: &str) -> Option<PendingReply> {
        let mut pending = self.inner.lock().await;
        let pending_reply = pending.remove(execution_id)?;

        for key in &pending_reply.keys {
            if key != execution_id {
                pending.remove(key);
            }
        }

        Some(pending_reply)
    }
}

/// Determines which keys a pending reply should be filed under: the
/// execution id from the request payload and, if different, the one derived
/// from the NATS subject it arrived on.
pub(super) fn pending_reply_keys(
    request_execution_id: &str,
    subject_execution_id: Option<&str>,
) -> Vec<String> {
    let mut keys = Vec::new();

    if !request_execution_id.is_empty() {
        keys.push(request_execution_id.to_string());
    }

    if let Some(subject_execution_id) = subject_execution_id
        && !subject_execution_id.is_empty()
        && !keys.iter().any(|key| key == subject_execution_id)
    {
        keys.push(subject_execution_id.to_string());
    }

    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_reply_keys_include_payload_and_subject_ids_once() {
        assert_eq!(
            pending_reply_keys("payload-id", Some("subject-id")),
            vec!["payload-id".to_string(), "subject-id".to_string()]
        );
        assert_eq!(
            pending_reply_keys("same-id", Some("same-id")),
            vec!["same-id".to_string()]
        );
        assert_eq!(
            pending_reply_keys("", Some("subject-id")),
            vec!["subject-id".to_string()]
        );
    }

    #[test]
    fn remove_removes_all_aliases() {
        futures::executor::block_on(async {
            let reply_subject = Subject::from("_INBOX.reply");
            let store = PendingReplyStore::new();

            store
                .insert(
                    reply_subject.clone(),
                    vec!["payload-id".to_string(), "subject-id".to_string()],
                )
                .await;

            let removed = store
                .remove("subject-id")
                .await
                .expect("pending reply should be found by alias");

            assert_eq!(removed.reply_subject, reply_subject);
            assert!(store.remove("payload-id").await.is_none());
        });
    }
}
