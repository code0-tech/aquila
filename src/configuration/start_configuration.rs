use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use clokwerk::{AsyncScheduler, TimeUnits};
use clokwerk::Interval::Seconds;
use log::{error, info};
use redis::aio::MultiplexedConnection;
use tokio::sync::Mutex;
use tokio::time::Interval;
use tucana_internal::sagittarius::Flow;
use tucana_internal::sagittarius::flow_service_client::FlowServiceClient;
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

#[async_trait]
impl StartConfiguration for StartConfigurationBase {
    async fn new(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>, config: Config) -> StartConfigurationBase {
        StartConfigurationBase { connection_arc, config }
    }

    async fn init_flows_from_sagittarius(&mut self) {
        let flow_service = FlowServiceBase::new(self.connection_arc.clone()).await;
        let flow_service_arc = Arc::new(Mutex::new(flow_service));
        let mut sagittarius_client = SagittariusFlowClientBase::new(self.config.backend_url.clone(), flow_service_arc).await;

        if !self.config.enable_scheduled_update {
            info!("Receiving flows from sagittarius once");
            sagittarius_client.send_start_request().await;
            return;
        }

        info!("Receiving flows from sagittarius on a scheduled basis");
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

    async fn init_flows_from_json(mut self) {
        if self.config.enable_grpc_update || self.config.enable_scheduled_update {
            return;
        }

        let mut flow_service = FlowServiceBase::new(self.connection_arc).await;
        let mut data = String::new();
        let mut file = File::open("configuration/configuration.json").expect("Cannot open file");

        file.read_to_string(&mut data).expect("Cannot read file");
        let flows: Vec<Flow> = serde_json::from_str(&data).expect("Failed to parse JSON to list of flows");
       
        info!("Loaded {} Flows!", &flows.len());
        flow_service.insert_flows(flows).await;
    }
}