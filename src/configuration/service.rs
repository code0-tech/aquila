use serde::{Deserialize, Serialize};
use serde_json::from_str;
use std::{fs::File, io::Read};
use tucana::shared::{ActionConfigurations, helper::value::from_json_value};

#[derive(Serialize, Deserialize, Clone)]
struct SerializableActionConfiguration {
    identifier: String,
    value: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
struct SerializeableActionProjectConfiguration {
    project_id: i64,
    #[serde(default)]
    configs: Vec<SerializableActionConfiguration>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializeableActionServiceConfiguration {
    token: String,
    identifier: String,
    #[serde(default)]
    configs: Vec<SerializeableActionProjectConfiguration>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SerializeableServiceConfiguration {
    #[serde(default)]
    actions: Vec<SerializeableActionServiceConfiguration>,
    #[serde(default)]
    runtimes: Vec<RuntimeServiceConfiguration>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ActionServiceConfiguration {
    token: String,
    service_name: String,
    config: Vec<ActionConfigurations>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RuntimeServiceConfiguration {
    token: String,
    identifier: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ServiceConfiguration {
    actions: Vec<ActionServiceConfiguration>,
    runtimes: Vec<RuntimeServiceConfiguration>,
}

impl From<SerializableActionConfiguration> for tucana::shared::ActionConfiguration {
    fn from(value: SerializableActionConfiguration) -> Self {
        Self {
            identifier: value.identifier,
            value: Some(from_json_value(value.value)),
        }
    }
}

impl From<SerializeableActionProjectConfiguration> for tucana::shared::ActionProjectConfiguration {
    fn from(value: SerializeableActionProjectConfiguration) -> Self {
        Self {
            project_id: value.project_id,
            action_configurations: value.configs.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<SerializeableActionServiceConfiguration> for ActionServiceConfiguration {
    fn from(value: SerializeableActionServiceConfiguration) -> Self {
        let action_identifier = value.identifier.clone();

        Self {
            token: value.token,
            service_name: value.identifier,
            config: vec![ActionConfigurations {
                action_identifier,
                action_configurations: value.configs.into_iter().map(Into::into).collect(),
            }],
        }
    }
}

impl From<SerializeableServiceConfiguration> for ServiceConfiguration {
    fn from(value: SerializeableServiceConfiguration) -> Self {
        Self {
            actions: value.actions.into_iter().map(Into::into).collect(),
            runtimes: value.runtimes.into_iter().map(Into::into).collect(),
        }
    }
}
impl ServiceConfiguration {
    pub fn has_service(&self, token: &String) -> bool {
        self.has_runtime(token) || self.has_action(token)
    }

    pub fn has_runtime(&self, token: &String) -> bool {
        match self.runtimes.iter().find(|x| &x.token == token) {
            Some(_) => true,
            None => false,
        }
    }

    pub fn has_action(&self, token: &String) -> bool {
        match self.actions.iter().find(|x| &x.token == token) {
            Some(_) => true,
            None => false,
        }
    }

    pub fn get_action_configuration(
        &self,
        action_identifier: &String,
    ) -> Vec<ActionConfigurations> {
        match self
            .actions
            .iter()
            .find(|x| &x.service_name == action_identifier)
        {
            Some(a) => a.config.clone(),
            None => vec![],
        }
    }

    pub fn from_path(path: &String) -> Self {
        let mut data = String::new();

        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) => {
                log::warn!(
                    "Couldn't open service configuration file, Reason: {}. Starting with empty service configuration",
                    error
                );
                return ServiceConfiguration::default();
            }
        };

        match file.read_to_string(&mut data) {
            Ok(_) => {
                log::debug!("Successfully loaded action configuration file");
            }
            Err(error) => {
                log::warn!(
                    "Couldn't read service configuration file, Reason: {}. Starting with empty service configuration",
                    error
                );
                return ServiceConfiguration::default();
            }
        }

        match from_str::<SerializeableServiceConfiguration>(&data) {
            Ok(conf) => return conf.into(),
            Err(error) => {
                log::warn!(
                    "Couldn't parse service configuration file, Reason: {}. Starting with empty service configuration",
                    error
                );
                return ServiceConfiguration::default();
            }
        };
    }
}
