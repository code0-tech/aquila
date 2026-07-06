use crate::{
    configuration::{config::Config, service::ServiceConfiguration, state::AppReadiness},
    flow::get_flow_identifier,
    server::static_server::AquilaStaticServer,
};
use async_nats::Client;
use prost::Message;
use serde_json::from_str;
use std::{fs::File, io::Read, sync::Arc, sync::atomic::Ordering};
use tucana::shared::Flows;

pub async fn run(
    config: Config,
    app_readiness: AppReadiness,
    service_config: ServiceConfiguration,

    client: Client,
    flow_store_client: Arc<async_nats::jetstream::kv::Store>,
) {
    log::info!(
        "Static mode starting grpc={}:{} fallback_path={}",
        config.grpc.host,
        config.grpc.port,
        config.static_config.flow_path
    );
    app_readiness
        .sagittarius_ready
        .store(true, Ordering::SeqCst);

    init_flows_from_json(
        config.static_config.flow_path.clone(),
        flow_store_client.clone(),
    )
    .await;

    let (action_config_tx, _) =
        tokio::sync::broadcast::channel::<tucana::shared::ModuleConfigurations>(64);

    let server = AquilaStaticServer::new(
        &config,
        app_readiness.clone(),
        service_config,
        client.clone(),
        flow_store_client.clone(),
        action_config_tx.clone(),
    );

    let mut server_task = tokio::spawn(async move {
        if let Err(err) = server.start().await {
            log::error!("gRPC server error: {:?}", err);
        } else {
            log::info!("gRPC server stopped gracefully");
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
                Err(err) => log::error!("gRPC server task failed; shutting down error={:?}", err),
            }
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Ctrl+C/Exit signal received, shutting down");
            server_task.abort();
        }
        _ = sigterm => {
            log::info!("SIGTERM received, shutting down");
            server_task.abort();
        }
    }

    log::info!("Aquila shutdown complete");
}

async fn init_flows_from_json(
    path: String,
    flow_store_client: Arc<async_nats::jetstream::kv::Store>,
) {
    let mut data = String::new();
    log::info!("Loading fallback flows from {}", path);

    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(error) => {
            panic!("Failed to open fallback flow file path={path}: {error:?}");
        }
    };

    match file.read_to_string(&mut data) {
        Ok(byte_count) => {
            log::debug!("Read fallback flow file path={} bytes={}", path, byte_count);
        }
        Err(error) => {
            panic!("Failed to read fallback flow file path={path}: {error:?}");
        }
    }

    let flows: Flows = match from_str(&data) {
        Ok(flows) => flows,
        Err(error) => {
            panic!("Failed to deserialize fallback flow file path={path}: {error:?}");
        }
    };

    let flow_count = flows.flows.len();
    if flow_count == 0 {
        log::warn!("Fallback flow file contains zero flows path={}", path);
    } else {
        log::info!(
            "Parsed fallback flow file path={} flow_count={}",
            path,
            flow_count
        );
    }

    let mut stored_count = 0;
    for flow in flows.flows {
        let key = get_flow_identifier(&flow);
        let bytes = flow.encode_to_vec();
        match flow_store_client.put(key.clone(), bytes.into()).await {
            Ok(_) => {
                stored_count += 1;
                log::debug!("Stored fallback flow key={}", key);
            }
            Err(err) => log::error!("Failed to store fallback flow key={} error={:?}", key, err),
        };
    }

    log::info!(
        "Finished loading fallback flows path={} parsed_count={} stored_count={}",
        path,
        flow_count,
        stored_count
    );
}
