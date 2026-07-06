use std::fmt;

use serde::{Deserialize, Serialize};

/// Controls whether flows are loaded locally or synchronized with Sagittarius.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Static,
    Dynamic,
}

impl fmt::Display for Mode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Static => "static",
            Self::Dynamic => "dynamic",
        };
        formatter.write_str(value)
    }
}
