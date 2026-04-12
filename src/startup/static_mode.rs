use crate::flow::get_flow_identifier;
use prost::Message;
use serde_json::from_str;
use std::{fs::File, io::Read, sync::Arc};
use tucana::shared::Flows;

pub async fn run(
    flow_fallback_path: String,
    flow_store_client: Arc<async_nats::jetstream::kv::Store>,
    mut server_task: tokio::task::JoinHandle<()>,
) {
    init_flows_from_json(flow_fallback_path, flow_store_client).await;

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

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            panic!("There was a problem opening the file: {:?}", error);
        }
    };

    match file.read_to_string(&mut data) {
        Ok(_) => {
            log::info!("Successfully read data from file");
        }
        Err(error) => {
            panic!("There was a problem reading the file: {:?}", error);
        }
    }

    let flows: Flows = match from_str(&data) {
        Ok(flows) => flows,
        Err(error) => {
            panic!(
                "There was a problem deserializing the json file: {:?}",
                error
            );
        }
    };

    for flow in flows.flows {
        let key = get_flow_identifier(&flow);
        let bytes = flow.encode_to_vec();
        log::info!("Inserting flow with key {}", &key);
        match flow_store_client.put(key, bytes.into()).await {
            Ok(_) => log::info!("Flow updated successfully"),
            Err(err) => log::error!("Failed to update flow. Reason: {:?}", err),
        };
    }

    log::info!("Successfully inserted all flows from the JSON file");
}
