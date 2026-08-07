//! Correlates an action-triggered flow execution with the gRPC stream to
//! deliver its result to, once a runtime reports it back.
//!
//! Unlike [`super::pending_replies::PendingReplyStore`] (which routes a
//! result back onto a NATS reply subject), a flow execution an action itself
//! asked Aquila to run needs its result delivered back over the *same*
//! `ActionTransferResponse` stream the action is already connected on - so
//! this registry stores the stream's sender directly instead.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::Mutex;
use tucana::aquila::ActionTransferResponse;

type ResponseSender = tokio::sync::mpsc::Sender<Result<ActionTransferResponse, tonic::Status>>;

/// Shared, lock-protected registry mapping execution identifiers to the
/// action stream a result must eventually be delivered to.
#[derive(Clone, Default)]
pub struct ActionFlowExecutionRegistry {
    inner: Arc<Mutex<HashMap<String, ResponseSender>>>,
}

impl ActionFlowExecutionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, execution_id: String, sender: ResponseSender) {
        self.inner.lock().await.insert(execution_id, sender);
    }

    /// Removes and returns the sender registered under `execution_id`, if any.
    pub async fn take(&self, execution_id: &str) -> Option<ResponseSender> {
        self.inner.lock().await.remove(execution_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_removes_the_registered_entry() {
        futures::executor::block_on(async {
            let registry = ActionFlowExecutionRegistry::new();
            let (tx, _rx) =
                tokio::sync::mpsc::channel::<Result<ActionTransferResponse, tonic::Status>>(1);

            registry.insert("execution-id".to_string(), tx).await;

            assert!(registry.take("execution-id").await.is_some());
            assert!(registry.take("execution-id").await.is_none());
        });
    }

    #[test]
    fn take_returns_none_for_unknown_execution_id() {
        futures::executor::block_on(async {
            let registry = ActionFlowExecutionRegistry::new();
            assert!(registry.take("missing").await.is_none());
        });
    }
}
