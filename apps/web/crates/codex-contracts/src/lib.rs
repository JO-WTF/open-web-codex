//! Generated Codex contract types and manifest negotiation for the Web platform.

mod generated;
mod manifest;

pub use generated::*;
pub use manifest::{
    NegotiatedCapability, NegotiationPolicy, NegotiationResult, negotiate_capability_manifest,
    parse_capability_manifest,
};
