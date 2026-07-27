//! Aquila's two run modes: [`Mode::Static`] serves a fixed local flow export
//! with no Sagittarius dependency, [`Mode::Dynamic`] syncs flows and module
//! state live over gRPC. See [`crate::startup::static_mode`] and
//! [`crate::startup::dynamic_mode`] for what each mode actually wires up.

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
