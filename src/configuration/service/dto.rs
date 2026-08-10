//! Wire format for the service configuration file (`AQUILA_SERVICE_CONFIG_PATH`)
//! and its conversion into the domain types in the parent module.
//!
//! The file format is intentionally a bit friendlier than the domain model:
//! actions are declared with a plain `identifier` and per-project `configs`,
//! then expanded here into the [`tucana::shared::ModuleConfigurations`]
//! shape the rest of Aquila works with. Runtimes need no such expansion, so
//! [`RuntimeServiceConfiguration`] doubles as both the wire format and the
//! domain type.

use serde::{Deserialize, Serialize};
use tucana::shared::{ModuleConfigurations, helper::value::from_json_value};

use super::{ActionServiceConfiguration, ServiceConfiguration};

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct SerializableModuleConfiguration {
    pub(super) identifier: String,
    pub(super) value: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct SerializableModuleProjectConfiguration {
    pub(super) project_id: i64,
    #[serde(default)]
    pub(super) configs: Vec<SerializableModuleConfiguration>,
}

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct SerializableActionServiceConfiguration {
    pub(super) token: String,
    pub(super) identifier: String,
    #[serde(default)]
    pub(super) configs: Vec<SerializableModuleProjectConfiguration>,
    /// How many `Split`-scaled connections this action identifier is expected
    /// to run as; see [`super::ActionServiceConfiguration`]. Defaults to `1`
    /// (a single connection receiving everything, same as `Disabled` scaling).
    #[serde(default = "default_replicas")]
    pub(super) replicas: u32,
}

fn default_replicas() -> u32 {
    1
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub(super) struct SerializableServiceConfiguration {
    #[serde(default)]
    pub(super) actions: Vec<SerializableActionServiceConfiguration>,
    #[serde(default)]
    pub(super) runtimes: Vec<RuntimeServiceConfiguration>,
}

/// A pre-provisioned runtime's token and identity. Used directly as both the
/// file format and the domain type since, unlike actions, no expansion is needed.
#[derive(Serialize, Deserialize, Clone)]
pub struct RuntimeServiceConfiguration {
    pub(super) token: String,
    pub(super) identifier: String,
    #[serde(default)]
    pub(super) resolved_modules: Vec<String>,
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
            replicas: value.replicas.max(1),
        }
    }
}

impl From<SerializableServiceConfiguration> for ServiceConfiguration {
    fn from(value: SerializableServiceConfiguration) -> Self {
        Self {
            actions: value.actions.into_iter().map(Into::into).collect(),
            runtimes: value.runtimes.into_iter().collect(),
        }
    }
}
