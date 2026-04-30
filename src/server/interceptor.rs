use crate::configuration::state::AppReadiness;
use std::sync::Arc;
use tonic::{Request, Status};

pub fn create_readiness_interceptor(
    readiness: Arc<AppReadiness>,
    dependency_name: &'static str,
) -> impl FnMut(Request<()>) -> Result<Request<()>, Status> + Clone {
    move |request: Request<()>| {
        if readiness.is_ready() {
            return Ok(request);
        }

        log::warn!(
            "Rejecting request because dependency={} is not ready",
            dependency_name
        );
        Err(Status::unavailable(format!(
            "service temporarily unavailable: {} is not ready",
            dependency_name
        )))
    }
}
