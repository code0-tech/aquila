//! The deployment environment Aquila believes it's running in, used to gate
//! environment-specific behavior (like the dev-only flow JSON export in
//! [`crate::sagittarius::flow_service_client_impl`]).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Which deployment environment Aquila is running in.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Development,
    Staging,
    Production,
}

impl fmt::Display for Environment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Development => "development",
            Self::Staging => "staging",
            Self::Production => "production",
        };
        formatter.write_str(value)
    }
}
