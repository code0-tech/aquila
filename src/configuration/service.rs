use serde::{Deserialize, Serialize};
use serde_json::from_str;
use std::{fs::File, io::Read};
use tucana::shared::{ModuleConfigurations, helper::value::from_json_value};

#[derive(Serialize, Deserialize, Clone)]
struct SerializableModuleConfiguration {
    identifier: String,
    value: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
struct SerializableModuleProjectConfiguration {
    project_id: i64,
    #[serde(default)]
    configs: Vec<SerializableModuleConfiguration>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableActionServiceConfiguration {
    token: String,
    identifier: String,
    #[serde(default)]
    configs: Vec<SerializableModuleProjectConfiguration>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SerializableServiceConfiguration {
    #[serde(default)]
    actions: Vec<SerializableActionServiceConfiguration>,
    #[serde(default)]
    runtimes: Vec<RuntimeServiceConfiguration>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ActionServiceConfiguration {
    token: String,
    service_name: String,
    config: Vec<ModuleConfigurations>,
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

impl From<SerializableModuleConfiguration> for tucana::shared::ModuleConfiguration {
    fn from(value: SerializableModuleConfiguration) -> Self {
        Self {
            identifier: value.identifier,
            value: Some(from_json_value(value.value)),
        }
    }
}

impl From<SerializableModuleProjectConfiguration> for tucana::shared::ModuleProjectConfigurations {
    fn from(value: SerializableModuleProjectConfiguration) -> Self {
        Self {
            project_id: value.project_id,
            module_configurations: value.configs.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<SerializableActionServiceConfiguration> for ActionServiceConfiguration {
    fn from(value: SerializableActionServiceConfiguration) -> Self {
        let module_identifier = value.identifier.clone();

        Self {
            token: value.token,
            service_name: value.identifier,
            config: vec![ModuleConfigurations {
                module_identifier,
                module_configurations: value.configs.into_iter().map(Into::into).collect(),
            }],
        }
    }
}

impl From<SerializableServiceConfiguration> for ServiceConfiguration {
    fn from(value: SerializableServiceConfiguration) -> Self {
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
    ) -> Vec<ModuleConfigurations> {
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

        match from_str::<SerializableServiceConfiguration>(&data) {
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
