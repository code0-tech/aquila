//! Domain model for Aquila's local service configuration file
//! (`AQUILA_SERVICE_CONFIG_PATH`): which action and runtime tokens are
//! pre-provisioned, and what configuration each action should receive.
//!
//! This is a static, file-backed allowlist — separate from the dynamic
//! module configuration Sagittarius pushes at runtime. See [`dto`] for the
//! on-disk format and how it's expanded into these types.

mod dto;

pub use dto::RuntimeServiceConfiguration;

use std::{fs::File, io::Read, path::Path};

use serde_json::from_str;
use tucana::shared::ModuleConfigurations;

use dto::SerializableServiceConfiguration;

#[derive(Clone)]
pub struct ActionServiceConfiguration {
    token: String,
    service_name: String,
    config: Vec<ModuleConfigurations>,
}

#[derive(Clone, Default)]
pub struct ServiceConfiguration {
    actions: Vec<ActionServiceConfiguration>,
    runtimes: Vec<RuntimeServiceConfiguration>,
}

impl ServiceConfiguration {
    /// Maps a runtime's advertised identifier to the family it belongs to,
    /// since individual `taurus-*` runtime instances all share one
    /// provisioned token under the `taurus` identifier while `draco-*`
    /// runtimes are provisioned individually.
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

        self.runtimes
            .iter()
            .find(|x| &x.token == token && x.identifier == name)
            .is_some()
    }

    pub fn has_action(&self, token: &String, action_name: &String) -> bool {
        self.actions
            .iter()
            .find(|x| &x.token == token && &x.service_name == action_name)
            .is_some()
    }

    pub fn get_action_configuration(
        &self,
        token: &String,
        action_identifier: &String,
    ) -> Vec<ModuleConfigurations> {
        match self
            .actions
            .iter()
            .find(|x| &x.token == token && &x.service_name == action_identifier)
        {
            Some(a) => a.config.clone(),
            None => vec![],
        }
    }

    /// Every module identifier Aquila should advertise as available,
    /// combining each action's own identifier with each runtime's resolved
    /// module list (or its own identifier, if it hasn't resolved any yet).
    pub fn collect_modules(&self) -> Vec<String> {
        let actions: Vec<String> = self
            .actions
            .iter()
            .map(|x| format!("action.{}", x.service_name))
            .collect();
        let runtime: Vec<String> = self
            .runtimes
            .iter()
            .flat_map(|x| match x.resolved_modules.is_empty() {
                true => vec![x.identifier.clone()],
                false => x.resolved_modules.clone(),
            })
            .collect();

        vec![actions, runtime].concat()
    }

    /// Loads the service configuration file at `path`, falling back to an
    /// empty configuration (rather than failing startup) if the file is
    /// missing, unreadable, or malformed — this file is optional.
    pub fn from_path(path: impl AsRef<Path>) -> Self {
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
                ServiceConfiguration::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RuntimeServiceConfiguration, ServiceConfiguration,
        dto::{
            SerializableActionServiceConfiguration, SerializableModuleConfiguration,
            SerializableModuleProjectConfiguration, SerializableServiceConfiguration,
        },
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
                    resolved_modules: vec![
                        String::from("taurus-boolean"),
                        String::from("taurus-number"),
                    ],
                },
                RuntimeServiceConfiguration {
                    token: String::from("draco-rest-token"),
                    identifier: String::from("draco-rest"),
                    resolved_modules: vec![],
                },
                RuntimeServiceConfiguration {
                    token: String::from("draco-cron-token"),
                    identifier: String::from("draco-cron"),
                    resolved_modules: vec![],
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
        assert!(config.has_runtime(&String::from("taurus-token"), &String::from("taurus")));
        assert!(config.has_runtime(
            &String::from("draco-rest-token"),
            &String::from("draco-rest")
        ));
        assert!(config.has_runtime(
            &String::from("draco-cron-token"),
            &String::from("draco-cron")
        ));
        assert!(!config.has_runtime(&String::from("taurus-token"), &String::from("draco-rest")));
        assert!(!config.has_runtime(&String::from("draco-rest-token"), &String::from("taurus-x")));
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
        assert!(!config.has_action(&String::from("action-token"), &String::from("action-other")));
        assert!(!config.has_action(&String::from("example"), &String::from("example")));
    }

    #[test]
    fn has_service_returns_true_for_valid_runtime_or_action_pairings() {
        let config = fixture();

        assert!(config.has_service(&String::from("taurus-token"), &String::from("taurus-x")));
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
        assert!(!config.has_service(&String::from("action-token"), &String::from("taurus-x")));
    }

    #[test]
    fn collect_modules_uses_definition_source_identifiers() {
        let config = fixture();

        assert_eq!(
            config.collect_modules(),
            vec![
                String::from("action.action-identifier"),
                String::from("taurus-boolean"),
                String::from("taurus-number"),
                String::from("draco-rest"),
                String::from("draco-cron"),
            ]
        );
    }

    #[test]
    fn get_action_configuration_requires_matching_token_and_identifier() {
        let config: ServiceConfiguration = SerializableServiceConfiguration {
            actions: vec![
                SerializableActionServiceConfiguration {
                    token: String::from("old-token"),
                    identifier: String::from("shared-action"),
                    configs: vec![SerializableModuleProjectConfiguration {
                        project_id: 1,
                        configs: vec![SerializableModuleConfiguration {
                            identifier: String::from("endpoint"),
                            value: serde_json::json!("old.example"),
                        }],
                    }],
                },
                SerializableActionServiceConfiguration {
                    token: String::from("new-token"),
                    identifier: String::from("shared-action"),
                    configs: vec![SerializableModuleProjectConfiguration {
                        project_id: 2,
                        configs: vec![SerializableModuleConfiguration {
                            identifier: String::from("endpoint"),
                            value: serde_json::json!("new.example"),
                        }],
                    }],
                },
            ],
            runtimes: vec![],
        }
        .into();

        let configs = config
            .get_action_configuration(&String::from("new-token"), &String::from("shared-action"));

        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].module_identifier, "shared-action");
        assert_eq!(configs[0].module_configurations[0].project_id, 2);
    }

    #[test]
    fn get_action_configuration_returns_empty_for_identifier_with_wrong_token() {
        let config = fixture();

        assert!(
            config
                .get_action_configuration(
                    &String::from("wrong-token"),
                    &String::from("action-identifier")
                )
                .is_empty()
        );
    }
}
