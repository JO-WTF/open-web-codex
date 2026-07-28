//! Typed catalog for code-published Supervisor Packages and Agent Definitions.
//!
//! The catalog declares stable capability relationships. Codex Runtime remains
//! authoritative for capability discovery and Agent execution.

pub mod agent;
pub mod supervisor;

mod seal;
mod validation;
