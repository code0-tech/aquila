//! Shared readiness flags consulted by [`crate::server::create_readiness_interceptor`]
//! to reject gRPC requests before Aquila's upstream dependencies are usable,
//! rather than accepting them and failing partway through a handler.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Tracks readiness of each external service.
#[derive(Clone)]
pub struct AppReadiness {
    /// Whether the Sagittarius gRPC channel has completed its initial connect.
    pub sagittarius_ready: Arc<AtomicBool>,
}

impl Default for AppReadiness {
    fn default() -> Self {
        Self::new()
    }
}

impl AppReadiness {
    pub fn new() -> Self {
        Self {
            sagittarius_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.sagittarius_ready.load(Ordering::SeqCst)
    }
}
