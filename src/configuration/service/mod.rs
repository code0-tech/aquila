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

use crate::authorization::authorization::verify_jwt;
use dto::SerializableServiceConfiguration;

#[derive(Clone)]
pub struct ActionServiceConfiguration {
    token: String,
    service_name: String,
    config: Vec<ModuleConfigurations>,
    /// How many `Split`-scaled connections this action identifier is
    /// expected to run as. Connections in `Disabled` scaling mode ignore
    /// this and each receive everything, same as if `replicas` were 1.
    replicas: u32,
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

    /// Resolves the runtime config entry `runtime_name` authenticates as,
    /// keyed by its family identifier (e.g. every `taurus-*` instance
    /// resolves to the one `taurus` entry) - and verified against `token`.
    /// Multiple entries may share an identifier (e.g. during a secret
    /// rotation window), so each is tried until one verifies.
    fn find_runtime(&self, token: &String, runtime_name: &String) -> Option<&RuntimeServiceConfiguration> {
        let name = Self::extract_service_name(runtime_name)?;
        self.runtimes
            .iter()
            .filter(|x| x.identifier == name)
            .find(|x| verify_jwt(token, &x.token, &x.identifier))
    }

    /// Resolves the action config entry registered under `action_name` and
    /// verified against `token` - see [`Self::find_runtime`] on why more
    /// than one entry may need to be tried.
    fn find_action(&self, token: &String, action_name: &String) -> Option<&ActionServiceConfiguration> {
        self.actions
            .iter()
            .filter(|x| &x.service_name == action_name)
            .find(|x| verify_jwt(token, &x.token, &x.service_name))
    }

    pub fn has_runtime(&self, token: &String, runtime_name: &String) -> bool {
        self.find_runtime(token, runtime_name).is_some()
    }

    pub fn has_action(&self, token: &String, action_name: &String) -> bool {
        self.find_action(token, action_name).is_some()
    }

    /// The configured replica count for a `token`/`action_identifier` pair,
    /// or `1` if the action isn't registered - a missing action never gets
    /// this far anyway, since [`Self::has_action`] gates logon first.
    pub fn action_replicas(&self, token: &String, action_identifier: &String) -> u32 {
        self.find_action(token, action_identifier)
            .map(|a| a.replicas)
            .unwrap_or(1)
    }

    pub fn get_action_configuration(
        &self,
        token: &String,
        action_identifier: &String,
    ) -> Vec<ModuleConfigurations> {
        match self.find_action(token, action_identifier) {
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

    /// Loads the service configuration file at `path`. A missing file is
    /// treated as "not configured" and falls back to an empty configuration,
    /// since this file is optional. But if the file exists and is unreadable
    /// or malformed, that's a real misconfiguration — silently falling back
    /// to an empty (deny-all) configuration there would mask it behind
    /// confusing "token not registered" rejections, so this returns an error
    /// for the caller to fail startup on instead.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let mut data = String::new();

        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) => {
                log::warn!(
                    "Couldn't open service configuration file, Reason: {}. Starting with empty service configuration",
                    error
                );
                return Ok(ServiceConfiguration::default());
            }
        };

        file.read_to_string(&mut data)
            .map_err(|error| format!("Couldn't read service configuration file: {}", error))?;

        log::debug!("Successfully loaded action configuration file");

        let config: ServiceConfiguration = from_str::<SerializableServiceConfiguration>(&data)
            .map(Into::into)
            .map_err(|error| format!("Couldn't parse service configuration file: {}", error))?;

        log::debug!(
            "Configured services: actions={:?}, runtimes={:?}",
            config
                .actions
                .iter()
                .map(|a| a.service_name.as_str())
                .collect::<Vec<_>>(),
            config
                .runtimes
                .iter()
                .map(|r| r.identifier.as_str())
                .collect::<Vec<_>>()
        );

        Ok(config)
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
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use serde::Serialize;

    #[derive(Serialize)]
    struct Claims {
        sub: String,
        exp: u64,
    }

    /// Mints a JWT the way an action/runtime client would, signed with
    /// `secret` (the value configured as that identity's `token`) and
    /// carrying `subject` (its identifier) as `sub`.
    fn jwt(secret: &str, subject: &str) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            &Claims {
                sub: subject.to_string(),
                exp: 4_102_444_800, // 2100-01-01, far enough out to never expire in tests
            },
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    fn fixture() -> ServiceConfiguration {
        SerializableServiceConfiguration {
            actions: vec![SerializableActionServiceConfiguration {
                token: String::from("action-secret"),
                identifier: String::from("action-identifier"),
                configs: vec![],
                replicas: 1,
            }],
            runtimes: vec![
                RuntimeServiceConfiguration {
                    token: String::from("taurus-secret"),
                    identifier: String::from("taurus"),
                    resolved_modules: vec![
                        String::from("taurus-boolean"),
                        String::from("taurus-number"),
                    ],
                },
                RuntimeServiceConfiguration {
                    token: String::from("draco-rest-secret"),
                    identifier: String::from("draco-rest"),
                    resolved_modules: vec![],
                },
                RuntimeServiceConfiguration {
                    token: String::from("draco-cron-secret"),
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
            &jwt("taurus-secret", "taurus"),
            &String::from("taurus-runtime-01")
        ));
        assert!(config.has_runtime(&jwt("taurus-secret", "taurus"), &String::from("taurus")));
        assert!(config.has_runtime(
            &jwt("draco-rest-secret", "draco-rest"),
            &String::from("draco-rest")
        ));
        assert!(config.has_runtime(
            &jwt("draco-cron-secret", "draco-cron"),
            &String::from("draco-cron")
        ));
        assert!(!config.has_runtime(
            &jwt("taurus-secret", "taurus"),
            &String::from("draco-rest")
        ));
        assert!(!config.has_runtime(
            &jwt("draco-rest-secret", "draco-rest"),
            &String::from("taurus-x")
        ));
        assert!(!config.has_runtime(
            &jwt("taurus-secret", "taurus"),
            &String::from("unknown-runtime")
        ));
    }

    #[test]
    fn has_runtime_rejects_wrong_secret_wrong_subject_or_plain_token() {
        let config = fixture();

        assert!(!config.has_runtime(
            &jwt("wrong-secret", "taurus"),
            &String::from("taurus-runtime-01")
        ));
        assert!(!config.has_runtime(
            &jwt("taurus-secret", "draco-rest"),
            &String::from("taurus-runtime-01")
        ));
        assert!(!config.has_runtime(
            &String::from("taurus-secret"),
            &String::from("taurus-runtime-01")
        ));
    }

    #[test]
    fn has_action_requires_matching_identifier_and_jwt() {
        let config = fixture();

        assert!(config.has_action(
            &jwt("action-secret", "action-identifier"),
            &String::from("action-identifier")
        ));
        assert!(!config.has_action(
            &jwt("taurus-secret", "taurus"),
            &String::from("action-identifier")
        ));
        assert!(!config.has_action(
            &jwt("action-secret", "action-identifier"),
            &String::from("action-other")
        ));
        assert!(!config.has_action(&String::from("action-secret"), &String::from("example")));
    }

    #[test]
    fn action_replicas_reads_the_configured_count_and_defaults_to_one() {
        let config: ServiceConfiguration = SerializableServiceConfiguration {
            actions: vec![SerializableActionServiceConfiguration {
                token: String::from("action-secret"),
                identifier: String::from("action-identifier"),
                configs: vec![],
                replicas: 3,
            }],
            runtimes: vec![],
        }
        .into();

        assert_eq!(
            config.action_replicas(
                &jwt("action-secret", "action-identifier"),
                &String::from("action-identifier")
            ),
            3
        );
        assert_eq!(
            config.action_replicas(&String::from("unknown"), &String::from("unknown")),
            1
        );
    }

    #[test]
    fn has_service_returns_true_for_valid_runtime_or_action_pairings() {
        let config = fixture();

        assert!(config.has_service(
            &jwt("taurus-secret", "taurus"),
            &String::from("taurus-x")
        ));
        assert!(config.has_service(
            &jwt("draco-rest-secret", "draco-rest"),
            &String::from("draco-rest")
        ));
        assert!(config.has_service(
            &jwt("action-secret", "action-identifier"),
            &String::from("action-identifier")
        ));
        assert!(!config.has_service(
            &jwt("draco-rest-secret", "draco-rest"),
            &String::from("action-identifier")
        ));
        assert!(!config.has_service(
            &jwt("action-secret", "action-identifier"),
            &String::from("taurus-x")
        ));
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
    fn get_action_configuration_requires_matching_jwt_and_identifier() {
        let config: ServiceConfiguration = SerializableServiceConfiguration {
            actions: vec![
                SerializableActionServiceConfiguration {
                    token: String::from("old-secret"),
                    identifier: String::from("shared-action"),
                    configs: vec![SerializableModuleProjectConfiguration {
                        project_id: 1,
                        configs: vec![SerializableModuleConfiguration {
                            identifier: String::from("endpoint"),
                            value: serde_json::json!("old.example"),
                        }],
                    }],
                    replicas: 1,
                },
                SerializableActionServiceConfiguration {
                    token: String::from("new-secret"),
                    identifier: String::from("shared-action"),
                    configs: vec![SerializableModuleProjectConfiguration {
                        project_id: 2,
                        configs: vec![SerializableModuleConfiguration {
                            identifier: String::from("endpoint"),
                            value: serde_json::json!("new.example"),
                        }],
                    }],
                    replicas: 1,
                },
            ],
            runtimes: vec![],
        }
        .into();

        // Both the old and the new secret authenticate during a rotation
        // window - each is a distinct config entry sharing the identifier.
        let old_configs = config.get_action_configuration(
            &jwt("old-secret", "shared-action"),
            &String::from("shared-action"),
        );
        assert_eq!(old_configs[0].module_configurations[0].project_id, 1);

        let new_configs = config.get_action_configuration(
            &jwt("new-secret", "shared-action"),
            &String::from("shared-action"),
        );
        assert_eq!(new_configs.len(), 1);
        assert_eq!(new_configs[0].module_identifier, "shared-action");
        assert_eq!(new_configs[0].module_configurations[0].project_id, 2);

        assert!(
            config
                .get_action_configuration(
                    &jwt("wrong-secret", "shared-action"),
                    &String::from("shared-action")
                )
                .is_empty()
        );
    }

    #[test]
    fn get_action_configuration_returns_empty_for_identifier_with_wrong_secret() {
        let config = fixture();

        assert!(
            config
                .get_action_configuration(
                    &jwt("wrong-secret", "action-identifier"),
                    &String::from("action-identifier")
                )
                .is_empty()
        );
    }
}
