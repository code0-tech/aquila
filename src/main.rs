use crate::{configuration::Config as AquilaConfig, flow::get_flow_identifier};
use async_nats::jetstream::kv::Config;
use code0_flow::flow_config::load_env_file;
use prost::Message;
use sagittarius::flow_service_client_impl::SagittariusFlowClient;
use serde_json::from_str;
use server::AquilaGRPCServer;
use std::{collections::HashMap, fs::File, io::Read, sync::Arc};
use tucana::shared::{
    FlowSetting, Flows, NodeFunction, NodeParameter, NodeValue, Struct, ValidationFlow, Value,
    node_value, value::Kind,
};

pub mod authorization;
pub mod configuration;
pub mod flow;
pub mod sagittarius;
pub mod server;
pub mod stream;

#[tokio::main]
async fn main() {
    let flow = ValidationFlow {
        flow_id: 1,
        project_id: 1,
        r#type: String::from("REST"),
        data_types: vec![],
        input_type_identifier: Some(String::from("HTTP_REQUEST")),
        return_type_identifier: Some(String::from("HTTP_RESPONSE")),
        settings: vec![
            FlowSetting {
                database_id: 1,
                flow_setting_id: String::from("HTTP_METHOD"),
                object: Some(Struct {
                    fields: HashMap::from([(
                        String::from("method"),
                        Value {
                            kind: Some(Kind::StringValue(String::from("GET"))),
                        },
                    )]),
                }),
            },
            FlowSetting {
                database_id: 1,
                flow_setting_id: String::from("HTTP_URL"),
                object: Some(Struct {
                    fields: HashMap::from([(
                        String::from("url"),
                        Value {
                            kind: Some(Kind::StringValue(String::from("/hello-world"))),
                        },
                    )]),
                }),
            },
            FlowSetting {
                database_id: 1,
                flow_setting_id: String::from("HTTP_HOST"),
                object: Some(Struct {
                    fields: HashMap::from([(
                        String::from("host"),
                        Value {
                            kind: Some(Kind::StringValue(String::from("localhost"))),
                        },
                    )]),
                }),
            },
        ],
        starting_node: Some(NodeFunction {
            database_id: 1,
            runtime_function_id: String::from("std::control::break"),
            next_node: None,
            parameters: vec![NodeParameter {
                database_id: 1,
                runtime_parameter_id: String::from("value"),
                value: Some(NodeValue {
                    value: Some(node_value::Value::LiteralValue(Value {
                        kind: Some(Kind::StructValue(Struct {
                            fields: HashMap::from([(
                                String::from("hallo"),
                                Value {
                                    kind: Some(Kind::StringValue(String::from("welt"))),
                                },
                            )]),
                        })),
                    })),
                }),
            }],
        }),
    };

    let s = serde_json::to_string_pretty(&flow).unwrap();
    println!("{}", s);

    log::info!("Starting Aquila...");

    // Configure Logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();

    // Load environment variables from .env file
    load_env_file();
    let config = AquilaConfig::new();

    //Create connection to JetStream
    let client = match async_nats::connect(config.nats_url.clone()).await {
        Ok(client) => client,
        Err(err) => panic!("Failed to connect to NATS server: {}", err),
    };

    let jetstream = async_nats::jetstream::new(client.clone());

    let _ = jetstream
        .create_key_value(Config {
            bucket: config.nats_bucket.clone(),
            ..Default::default()
        })
        .await;

    let kv_store = match jetstream.get_key_value(config.nats_bucket.clone()).await {
        Ok(kv) => Arc::new(kv),
        Err(err) => panic!("Failed to get key-value store: {}", err),
    };

    //Create connection to Sagittarius if the type is hybrid
    if !config.is_static() {
        let server = AquilaGRPCServer::new(&config);

        match server.start().await {
            Ok(_) => {
                log::info!("Server started successfully");
            }
            Err(err) => {
                log::error!("Failed to start server: {:?}", err);
                panic!("Failed to start server");
            }
        };

        let mut sagittarius_client =
            SagittariusFlowClient::new(config.backend_url, kv_store, config.runtime_token).await;

        sagittarius_client.init_flow_stream().await;
    } else {
        init_flows_from_json(config.flow_fallback_path, kv_store).await
    }
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
            print!("Successfully read data from file");
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
}
