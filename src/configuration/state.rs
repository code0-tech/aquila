use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Tracks readiness of each external service.
#[derive(Clone)]
pub struct AppReadiness {
    // Readiness state of Sagittarius service
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
