//! Typed catalog for code-published Supervisor Packages and Agent Definitions.
//!
//! The catalog declares stable capability relationships. Codex Runtime remains
//! authoritative for capability discovery and Agent execution.

pub mod agent;
pub mod capability_package;
pub mod instruction_policy;
pub mod semantics;
pub mod supervisor;

mod agent_release;
mod seal;
mod validation;
