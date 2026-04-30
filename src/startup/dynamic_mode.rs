use async_nats::Client;

use crate::{
    configuration::{
        config::Config as AquilaConfig, service::ServiceConfiguration, state::AppReadiness,
    },
    sagittarius::{
        flow_service_client_impl::SagittariusFlowClient, retry::create_channel_with_retry,
        test_execution_client_impl::SagittariusTestExecutionServiceClient,
    },
    server::dynamic_server::AquilaDynamicServer,
};
use std::{sync::Arc, time::Duration};

pub async fn run(
    config: AquilaConfig,
    app_readiness: AppReadiness,
    service_config: ServiceConfiguration,
    client: Client,
    kv_store: Arc<async_nats::jetstream::kv::Store>,
) {
    log::info!(
        "Dynamic mode starting grpc={}:{} backend_url={}",
        config.grpc_host,
        config.grpc_port,
        config.backend_url
    );

    let backend_url_flow = config.backend_url.clone();
    let sagittarius_channel = create_channel_with_retry(
        "Sagittarius Endpoint",
        backend_url_flow,
        app_readiness.sagittarius_ready.clone(),
    )
    .await;

    let (action_config_tx, _) =
        tokio::sync::broadcast::channel::<tucana::shared::ModuleConfigurations>(64);

    let server = AquilaDynamicServer::new(
        &config,
        app_readiness.clone(),
        sagittarius_channel.clone(),
        service_config,
        client.clone(),
        kv_store.clone(),
        action_config_tx.clone(),
    );

    let mut server_task = tokio::spawn(async move {
        if let Err(err) = server.start().await {
            log::error!("gRPC server error: {:?}", err);
        } else {
            log::info!("gRPC server stopped gracefully");
        }
    });

    let kv_for_test_execution = kv_store.clone();
    let kv_for_flow = kv_store.clone();
    let backend_url_for_test_execution = config.backend_url.clone();
    let runtime_token_for_test_execution = config.runtime_token.clone();
    let sagittarius_ready_for_test_execution = app_readiness.sagittarius_ready.clone();
    let nats_client_for_test_execution = client.clone();

    let backend_url_for_flow = config.backend_url.clone();
    let runtime_token_for_flow = config.runtime_token.clone();
    let sagittarius_ready_for_flow = app_readiness.sagittarius_ready.clone();

    let env = match config.environment {
        code0_flow::flow_config::environment::Environment::Development => {
            String::from("DEVELOPMENT")
        }
        code0_flow::flow_config::environment::Environment::Staging => String::from("STAGING"),
        code0_flow::flow_config::environment::Environment::Production => String::from("PRODUCTION"),
    };

    let mut test_execution_task = tokio::spawn(async move {
        let mut backoff = Duration::from_millis(200);
        let max_backoff = Duration::from_secs(10);

        loop {
            log::debug!(
                "Attempting to initialize Sagittarius test execution stream backoff_ms={}",
                backoff.as_millis()
            );
            let ch = create_channel_with_retry(
                "Sagittarius Test Execution Stream",
                backend_url_for_test_execution.clone(),
                sagittarius_ready_for_test_execution.clone(),
            )
            .await;

            let mut test_execution_client = SagittariusTestExecutionServiceClient::new(
                nats_client_for_test_execution.clone(),
                kv_for_test_execution.clone(),
                ch,
                runtime_token_for_test_execution.clone(),
            );

            test_execution_client.logon().await;
            tokio::time::sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, max_backoff);
            log::debug!(
                "Next test execution stream reconnect backoff_ms={}",
                backoff.as_millis()
            );
        }
    });

    let mut flow_task = tokio::spawn(async move {
        let mut backoff = Duration::from_millis(200);
        let max_backoff = Duration::from_secs(10);

        loop {
            log::debug!(
                "Attempting to initialize Sagittarius flow stream backoff_ms={}",
                backoff.as_millis()
            );
            let ch = create_channel_with_retry(
                "Sagittarius Stream",
                backend_url_for_flow.clone(),
                sagittarius_ready_for_flow.clone(),
            )
            .await;

            let mut flow_client = SagittariusFlowClient::new(
                kv_for_flow.clone(),
                env.clone(),
                runtime_token_for_flow.clone(),
                ch,
                sagittarius_ready_for_flow.clone(),
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
            log::debug!(
                "Next flow stream reconnect backoff_ms={}",
                backoff.as_millis()
            );
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
            test_execution_task.abort();
        }
        _ = &mut test_execution_task => {
            log::warn!("Test execution stream task finished, shutting down");
            server_task.abort();
            flow_task.abort();
        }
        _ = &mut flow_task => {
            log::warn!("Flow stream task finished, shutting down");
            server_task.abort();
            test_execution_task.abort();
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Ctrl+C/Exit signal received, shutting down");
            server_task.abort();
            flow_task.abort();
            test_execution_task.abort();
        }
        _ = sigterm => {
            log::info!("SIGTERM received, shutting down");
            server_task.abort();
            flow_task.abort();
            test_execution_task.abort();
        }
    }

    log::info!("Aquila shutdown complete");
}
