use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use clokwerk::{AsyncScheduler, TimeUnits};
use clokwerk::Interval::Seconds;
use log::{debug, error, info};
use redis::aio::MultiplexedConnection;
use tokio::sync::Mutex;
use tokio::time::Interval;
use tucana::sagittarius::Flow;
use tucana::sagittarius::flow_service_client::FlowServiceClient;
use crate::client::sagittarius::flow_client::{SagittariusFlowClient, SagittariusFlowClientBase};
use crate::configuration::config::Config;
use crate::service::flow_service::{FlowService, FlowServiceBase};

pub struct StartConfigurationBase {
    connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
    config: Config,
}

#[async_trait]
pub trait StartConfiguration {
    async fn new(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>, config: Config) -> StartConfigurationBase;
    async fn init_flows_from_sagittarius(&mut self);
    async fn init_flows_from_json(mut self);
}

/// `Aquila's` startup configuration logic.
#[async_trait]
impl StartConfiguration for StartConfigurationBase {
    async fn new(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>, config: Config) -> StartConfigurationBase {
        StartConfigurationBase { connection_arc, config }
    }

    /// Function to initialize the connection to `Sagittarius` to receive latest flows.
    ///
    /// Behavior:
    /// If scheduling is disabled a request will be sent once.
    /// If scheduling is enabled a scheduler will start to send request to `Sagittarius` by its configured interval.
    ///
    /// Will panic when:
    /// - `Sagittarius` connection buildup fails
    /// - Redis connection buildup fails
    async fn init_flows_from_sagittarius(&mut self) {
        let flow_service = FlowServiceBase::new(self.connection_arc.clone()).await;
        let flow_service_arc = Arc::new(Mutex::new(flow_service));
        let mut sagittarius_client = SagittariusFlowClientBase::new(self.config.backend_url.clone(), flow_service_arc).await;

        if !self.config.enable_scheduled_update {
            info!("Receiving flows from sagittarius once");
            sagittarius_client.send_start_request().await;
            return;
        }

        info!("Receiving flows from sagittarius on a scheduled basis.");
        let schedule_interval = self.config.update_schedule_interval;
        let mut scheduler = AsyncScheduler::new();

        scheduler
            .every(Seconds(schedule_interval))
            .run(move || {
                let local_flow_client = Arc::new(Mutex::new(sagittarius_client.clone()));

                async move {
                    let mut current_flow_client = local_flow_client.lock().await;
                    current_flow_client.send_start_request().await
                }
            });
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
        if self.config.enable_grpc_update || self.config.enable_scheduled_update {
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

        let flows: Vec<Flow> = match serde_json::from_str(&data) {
            Ok(flows) => flows,
            Err(error) => {
                error!("Error deserializing json file {}", error);
                panic!("There was a problem deserializing the json file: {:?}", error);
            }
        };

        info!("Loaded {} Flows!", &flows.len());
        flow_service.insert_flows(flows).await;
    }
}