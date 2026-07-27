//! Everything Aquila loads before it can start serving: the process-wide
//! [`config::Config`] (file + env), the static [`service::ServiceConfiguration`]
//! allowlist, and small shared value types ([`env::Environment`], [`mode::Mode`],
//! [`state::AppReadiness`]) used across both.

pub mod config;
pub mod env;
pub mod mode;
pub mod service;
pub mod state;
