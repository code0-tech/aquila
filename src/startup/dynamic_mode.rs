use crate::{
    configuration::{config::Config as AquilaConfig, state::AppReadiness},
    sagittarius::{
        flow_service_client_impl::SagittariusFlowClient, retry::create_channel_with_retry,
    },
};
use std::{sync::Arc, time::Duration};

pub async fn run(
    config: AquilaConfig,
    app_readiness: AppReadiness,
    kv_store: Arc<async_nats::jetstream::kv::Store>,
    action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ActionConfigurations>,
    mut server_task: tokio::task::JoinHandle<()>,
) {
    let kv_for_flow = kv_store.clone();

    let env = match config.environment {
        code0_flow::flow_config::environment::Environment::Development => {
            String::from("DEVELOPMENT")
        }
        code0_flow::flow_config::environment::Environment::Staging => String::from("STAGING"),
        code0_flow::flow_config::environment::Environment::Production => String::from("PRODUCTION"),
    };

    let mut flow_task = tokio::spawn(async move {
        let mut backoff = Duration::from_millis(200);
        let max_backoff = Duration::from_secs(10);

        loop {
            let ch = create_channel_with_retry(
                "Sagittarius Stream",
                config.backend_url.clone(),
                app_readiness.sagittarius_ready.clone(),
            )
            .await;

            let mut flow_client = SagittariusFlowClient::new(
                kv_for_flow.clone(),
                env.clone(),
                config.runtime_token.clone(),
                ch,
                app_readiness.sagittarius_ready.clone(),
                action_config_tx.clone(),
            );

            match flow_client.init_flow_stream().await {
                Ok(_) => {
                    log::warn!("Flow stream ended cleanly. Reconnecting...");
                }
                Err(e) => {
                    log::warn!("Flow stream dropped: {:?}. Reconnecting...", e);
                }
            }

            tokio::time::sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, max_backoff);
        }
    });

    #[cfg(unix)]
    let sigterm = async {
        use tokio::signal::unix::{SignalKind, signal};

        let mut term = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        term.recv().await;
    };

    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    tokio::select! {
        _ = &mut server_task => {
            log::warn!("gRPC server task finished, shutting down");
            flow_task.abort();
        }
        _ = &mut flow_task => {
            log::warn!("Flow stream task finished, shutting down");
            server_task.abort();
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Ctrl+C/Exit signal received, shutting down");
            server_task.abort();
            flow_task.abort();
        }
        _ = sigterm => {
            log::info!("SIGTERM received, shutting down");
            server_task.abort();
            flow_task.abort();
        }
    }

    log::info!("Aquila shutdown complete");
}
