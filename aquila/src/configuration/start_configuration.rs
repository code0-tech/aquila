use crate::client::sagittarius::flow_client::{SagittariusFlowClient, SagittariusFlowClientBase};
use crate::configuration::config::Config;
use crate::configuration::mode::Mode;
use aquila_store::{FlowService, FlowServiceBase};
use async_trait::async_trait;
use log::{debug, error, info};
use redis::aio::MultiplexedConnection;
use serde_json::from_str;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use tokio::sync::Mutex;
use tucana::sagittarius::Flow;

pub struct StartConfigurationBase {
    connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
    config: Config,
}

#[async_trait]
pub trait StartConfiguration {
    async fn new(
        connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
        config: Config,
    ) -> StartConfigurationBase;
    async fn init_flows_from_sagittarius(&mut self);
    async fn init_flows_from_json(mut self);
}

/// `Aquila's` startup configuration logic.
#[async_trait]
impl StartConfiguration for StartConfigurationBase {
    async fn new(
        connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
        config: Config,
    ) -> StartConfigurationBase {
        StartConfigurationBase {
            connection_arc,
            config,
        }
    }

    /// Function to initialize the connection to `Sagittarius` to receive flows.
    ///
    /// Behavior:
    /// If scheduling is disabled a request will be sent once.
    /// If scheduling is enabled a scheduler will start to send request to `Sagittarius` by its configured interval.
    ///
    /// Will panic when:
    /// - `Sagittarius` connection buildup fails
    /// - Redis connection buildup fails
    async fn init_flows_from_sagittarius(&mut self) {
        if &self.config.mode != Mode::DYNAMIC {
            return;
        }

        let flow_service = FlowServiceBase::new(self.connection_arc.clone()).await;
        let flow_service_arc = Arc::new(Mutex::new(flow_service));
        let mut sagittarius_client =
            SagittariusFlowClientBase::new(self.config.backend_url.clone(), flow_service_arc).await;

        sagittarius_client.init_flow_stream().await
    }

    /// Function to start `Aquila` from a JSON containing the flows.
    ///
    /// Behavior
    /// If gRPC & Scheduling is disabled `Aquila` will search for a flow file.
    /// Flow will be found by the configured route.
    ///
    /// Will panic when:
    /// - Redis connection buildup fails
    /// - File is not found
    /// - File is not readable
    /// - File is not parsable
    async fn init_flows_from_json(mut self) {
        if &self.config.mode != Mode::STATIC {
            return;
        }

        let mut flow_service = FlowServiceBase::new(self.connection_arc).await;
        let path = self.config.flow_fallback_path.as_str();
        let mut data = String::new();

        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) => {
                error!("Error opening file {}", error);
                panic!("There was a problem opening the file: {:?}", error);
            }
        };

        match file.read_to_string(&mut data) {
            Ok(_) => {
                debug!("Successfully read data from file");
            }
            Err(error) => {
                error!("Error reading file {}", error);
                panic!("There was a problem reading the file: {:?}", error);
            }
        }

        let flows: Vec<Flow> = match from_str(&data) {
            Ok(flows) => flows,
            Err(error) => {
                error!("Error deserializing json file {}", error);
                panic!(
                    "There was a problem deserializing the json file: {:?}",
                    error
                );
            }
        };

        info!("Loaded {} Flows!", &flows.len());
        flow_service.insert_flows(flows).await;
    }
}
