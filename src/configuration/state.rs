use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Tracks readiness of each external service.
#[derive(Clone)]
pub struct AppReadiness {
    // Readieness State of Sagjttarus Service
    pub sagittarius_ready: Arc<AtomicBool>,
}

impl AppReadiness {
    pub fn new() -> Self {
        Self {
            sagittarius_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Synchronously returns true if *all* flags are true.
    pub fn is_ready(&self) -> bool {
        self.sagittarius_ready.load(Ordering::SeqCst)
    }
}

