//! Dynamic mode wiring: the gRPC server and three independent, self-healing
//! Sagittarius streams (flow sync, module configuration sync, test
//! execution) all run as separate tasks supervised by a single `select!` —
//! if any one of them exits or panics, the others are aborted and Aquila
//! shuts down rather than continuing in a partially working state.

use async_nats::Client;

use crate::{
    configuration::{
        config::Config as AquilaConfig, service::ServiceConfiguration, state::AppReadiness,
    },
    sagittarius::{
        flow_service_client_impl::SagittariusFlowClient,
        module_configuration_client_impl::SagittariusModuleConfigurationClient,
        retry::create_channel_with_retry,
        test_execution_client_impl::{
            SagittariusExecutionResponseSender, SagittariusTestExecutionServiceClient,
        },
    },
    server::dynamic_server::{AquilaDynamicServer, DynamicServerDependencies},
    telemetry::errors,
};
use std::{sync::Arc, time::Duration};

/// Starts the gRPC server plus the flow-sync and test-execution stream
/// tasks, and blocks until one of them exits, panics, or a shutdown signal
/// arrives.
pub async fn run(
    config: AquilaConfig,
    app_readiness: AppReadiness,
    service_config: ServiceConfiguration,
    client: Client,
    kv_store: Arc<async_nats::jetstream::kv::Store>,
) {
    log::info!(
        "Dynamic mode starting grpc={}:{} backend_url={}",
        config.grpc.host,
        config.grpc.port,
        config.dynamic_config.backend_url
    );

    let backend_url_flow = config.dynamic_config.backend_url.clone();
    let sagittarius_channel = create_channel_with_retry(
        "Sagittarius Endpoint",
        backend_url_flow,
        app_readiness.sagittarius_ready.clone(),
    )
    .await;

    let (action_config_tx, _) =
        tokio::sync::broadcast::channel::<tucana::shared::ModuleConfigurations>(64);
    let (action_flow_tx, _) = tokio::sync::broadcast::channel::<crate::flow::FlowChange>(64);
    let execution_response_sender = SagittariusExecutionResponseSender::new();

    let server = AquilaDynamicServer::new(
        &config,
        DynamicServerDependencies {
            app_readiness: app_readiness.clone(),
            channel: sagittarius_channel.clone(),
            service_configuration: service_config,
            nats_client: client.clone(),
            kv_store: kv_store.clone(),
            action_config_tx: action_config_tx.clone(),
            action_flow_tx: action_flow_tx.clone(),
            execution_response_sender: execution_response_sender.clone(),
        },
    );

    let mut server_task = tokio::spawn(async move {
        if let Err(err) = server.start().await {
            errors::record("server", "grpc.serve", &err, "mode=dynamic");
        } else {
            log::info!("gRPC server stopped gracefully");
        }
    });

    let kv_for_test_execution = kv_store.clone();
    let kv_for_flow = kv_store.clone();
    let backend_url_for_test_execution = config.dynamic_config.backend_url.clone();
    let runtime_token_for_test_execution = config.dynamic_config.backend_token.clone();
    let sagittarius_ready_for_test_execution = app_readiness.sagittarius_ready.clone();
    let nats_client_for_test_execution = client.clone();
    let execution_response_sender_for_test_execution = execution_response_sender.clone();

    let backend_url_for_flow = config.dynamic_config.backend_url.clone();
    let runtime_token_for_flow = config.dynamic_config.backend_token.clone();
    let flow_export_path_for_flow = config.static_config.flow_path.clone();
    let sagittarius_ready_for_flow = app_readiness.sagittarius_ready.clone();
    let action_flow_tx_for_flow = action_flow_tx.clone();

    let backend_url_for_module_configuration = config.dynamic_config.backend_url.clone();
    let runtime_token_for_module_configuration = config.dynamic_config.backend_token.clone();
    let sagittarius_ready_for_module_configuration = app_readiness.sagittarius_ready.clone();
    let action_config_tx_for_module_configuration = action_config_tx.clone();

    let env = match config.environment {
        crate::configuration::env::Environment::Development => String::from("DEVELOPMENT"),
        crate::configuration::env::Environment::Staging => String::from("STAGING"),
        crate::configuration::env::Environment::Production => String::from("PRODUCTION"),
    };

    // Both stream tasks below share the same shape: connect, run the stream
    // until it ends (which `logon`/`init_flow_stream` always eventually do,
    // since Sagittarius connections aren't permanent), then reconnect with
    // growing backoff. The task itself never exits on its own.
    let mut test_execution_task = tokio::spawn(async move {
        let mut backoff = Duration::from_millis(200);
        let max_backoff = Duration::from_secs(10);

        loop {
            log::debug!(
                "Attempting to initialize Sagittarius execution stream backoff_ms={}",
                backoff.as_millis()
            );
            let ch = create_channel_with_retry(
                "Sagittarius Execution Stream",
                backend_url_for_test_execution.clone(),
                sagittarius_ready_for_test_execution.clone(),
            )
            .await;

            let mut test_execution_client = SagittariusTestExecutionServiceClient::new(
                nats_client_for_test_execution.clone(),
                kv_for_test_execution.clone(),
                ch,
                runtime_token_for_test_execution.clone(),
                execution_response_sender_for_test_execution.clone(),
            );

            test_execution_client.logon().await;
            tokio::time::sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, max_backoff);
            log::debug!(
                "Next execution stream reconnect backoff_ms={}",
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
                flow_export_path_for_flow.clone(),
                ch,
                sagittarius_ready_for_flow.clone(),
                action_flow_tx_for_flow.clone(),
            );

            match flow_client.init_flow_stream().await {
                Ok(_) => {
                    log::warn!(
                        "Sagittarius flow synchronization stream ended normally; reconnecting"
                    );
                }
                Err(e) => {
                    log::warn!(
                        "Sagittarius flow synchronization stream dropped; reconnecting error={:?}",
                        e
                    );
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

    let mut module_configuration_task = tokio::spawn(async move {
        let mut backoff = Duration::from_millis(200);
        let max_backoff = Duration::from_secs(10);

        loop {
            log::debug!(
                "Attempting to initialize Sagittarius module configuration stream backoff_ms={}",
                backoff.as_millis()
            );
            let ch = create_channel_with_retry(
                "Sagittarius Module Configuration Stream",
                backend_url_for_module_configuration.clone(),
                sagittarius_ready_for_module_configuration.clone(),
            )
            .await;

            let mut module_configuration_client = SagittariusModuleConfigurationClient::new(
                ch,
                runtime_token_for_module_configuration.clone(),
                action_config_tx_for_module_configuration.clone(),
            );

            match module_configuration_client
                .init_configuration_stream()
                .await
            {
                Ok(_) => {
                    log::warn!(
                        "Sagittarius module configuration stream ended normally; reconnecting"
                    );
                }
                Err(e) => {
                    log::warn!(
                        "Sagittarius module configuration stream dropped; reconnecting error={:?}",
                        e
                    );
                }
            }

            tokio::time::sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, max_backoff);
            log::debug!(
                "Next module configuration stream reconnect backoff_ms={}",
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
        result = &mut server_task => {
            match result {
                Ok(()) => log::warn!("gRPC server task exited unexpectedly; shutting down"),
                Err(err) if err.is_panic() => {}
                Err(err) => errors::record("task", "grpc.task", &err, "mode=dynamic"),
            }
            flow_task.abort();
            module_configuration_task.abort();
            test_execution_task.abort();
        }
        result = &mut test_execution_task => {
            match result {
                Ok(()) => log::warn!("Test execution stream task exited unexpectedly; shutting down"),
                Err(err) if err.is_panic() => {}
                Err(err) => errors::record("task", "execution_stream.task", &err, "mode=dynamic"),
            }
            server_task.abort();
            flow_task.abort();
            module_configuration_task.abort();
        }
        result = &mut flow_task => {
            match result {
                Ok(()) => log::warn!("Flow stream task exited unexpectedly; shutting down"),
                Err(err) if err.is_panic() => {}
                Err(err) => errors::record("task", "flow_stream.task", &err, "mode=dynamic"),
            }
            server_task.abort();
            module_configuration_task.abort();
            test_execution_task.abort();
        }
        result = &mut module_configuration_task => {
            match result {
                Ok(()) => log::warn!("Module configuration stream task exited unexpectedly; shutting down"),
                Err(err) if err.is_panic() => {}
                Err(err) => errors::record("task", "module_configuration_stream.task", &err, "mode=dynamic"),
            }
            server_task.abort();
            flow_task.abort();
            test_execution_task.abort();
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Ctrl+C/Exit signal received, shutting down");
            server_task.abort();
            flow_task.abort();
            module_configuration_task.abort();
            test_execution_task.abort();
        }
        _ = sigterm => {
            log::info!("SIGTERM received, shutting down");
            server_task.abort();
            flow_task.abort();
            module_configuration_task.abort();
            test_execution_task.abort();
        }
    }

    log::info!("Aquila shutdown complete");
}
