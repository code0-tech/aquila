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
    // This function is used to extract the real service name via modules
    pub fn extract_service_name(name: &String) -> Option<String> {
        if name.starts_with("draco") {
            return Some(name.clone());
        };

        if name.starts_with("taurus") {
            return Some(String::from("taurus"));
        };

        None
    }

    pub fn has_service(&self, token: &String, name: &String) -> bool {
        self.has_runtime(token, name) || self.has_action(token, name)
    }

    pub fn has_runtime(&self, token: &String, runtime_name: &String) -> bool {
        let name = match Self::extract_service_name(runtime_name) {
            Some(n) => n,
            None => return false,
        };

        match self
            .runtimes
            .iter()
            .find(|x| &x.token == token && x.identifier == name)
        {
            Some(_) => true,
            None => false,
        }
    }

    pub fn has_action(&self, token: &String, action_name: &String) -> bool {
        match self
            .actions
            .iter()
            .find(|x| &x.token == token && &x.service_name == action_name)
        {
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

#[cfg(test)]
mod tests {
    use super::{
        RuntimeServiceConfiguration, SerializableActionServiceConfiguration,
        SerializableServiceConfiguration, ServiceConfiguration,
    };

    fn fixture() -> ServiceConfiguration {
        SerializableServiceConfiguration {
            actions: vec![SerializableActionServiceConfiguration {
                token: String::from("action-token"),
                identifier: String::from("action-identifier"),
                configs: vec![],
            }],
            runtimes: vec![
                RuntimeServiceConfiguration {
                    token: String::from("taurus-token"),
                    identifier: String::from("taurus"),
                },
                RuntimeServiceConfiguration {
                    token: String::from("draco-rest-token"),
                    identifier: String::from("draco-rest"),
                },
                RuntimeServiceConfiguration {
                    token: String::from("draco-cron-token"),
                    identifier: String::from("draco-cron"),
                },
            ],
        }
        .into()
    }

    #[test]
    fn has_runtime_matches_taurus_aliases_and_draco_identifiers() {
        let config = fixture();

        assert!(config.has_runtime(
            &String::from("taurus-token"),
            &String::from("taurus-runtime-01")
        ));
        assert!(config.has_runtime(
            &String::from("taurus-token"),
            &String::from("taurus")
        ));
        assert!(config.has_runtime(
            &String::from("draco-rest-token"),
            &String::from("draco-rest")
        ));
        assert!(config.has_runtime(
            &String::from("draco-cron-token"),
            &String::from("draco-cron")
        ));
        assert!(!config.has_runtime(
            &String::from("taurus-token"),
            &String::from("draco-rest")
        ));
        assert!(!config.has_runtime(
            &String::from("draco-rest-token"),
            &String::from("taurus-x")
        ));
        assert!(!config.has_runtime(
            &String::from("taurus-token"),
            &String::from("unknown-runtime")
        ));
    }

    #[test]
    fn has_action_requires_exact_identifier_and_matching_token() {
        let config = fixture();

        assert!(config.has_action(
            &String::from("action-token"),
            &String::from("action-identifier")
        ));
        assert!(!config.has_action(
            &String::from("taurus-token"),
            &String::from("action-identifier")
        ));
        assert!(!config.has_action(
            &String::from("action-token"),
            &String::from("action-other")
        ));
    }

    #[test]
    fn has_service_returns_true_for_valid_runtime_or_action_pairings() {
        let config = fixture();

        assert!(config.has_service(
            &String::from("taurus-token"),
            &String::from("taurus-x")
        ));
        assert!(config.has_service(
            &String::from("draco-rest-token"),
            &String::from("draco-rest")
        ));
        assert!(config.has_service(
            &String::from("action-token"),
            &String::from("action-identifier")
        ));
        assert!(!config.has_service(
            &String::from("draco-rest-token"),
            &String::from("action-identifier")
        ));
        assert!(!config.has_service(
            &String::from("action-token"),
            &String::from("taurus-x")
        ));
    }
}
