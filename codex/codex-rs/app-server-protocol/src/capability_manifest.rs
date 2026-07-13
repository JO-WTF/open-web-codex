use std::collections::HashMap;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;


/// Top-level capability manifest returned during `initialize`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityManifest {
    pub schema_version: String,
    pub generated_at: String,
    pub server: ServerIdentity,
    pub compatibility: CompatibilityRange,
    #[serde(default)]
    pub capabilities: Vec<CapabilityDeclaration>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ServerIdentity {
    pub protocol_version: String,
    pub build_version: String,
    pub commit: String,
    pub target: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityRange {
    pub minimum_client_protocol: String,
    pub maximum_client_protocol: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDeclaration {
    pub id: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub status: CapabilityStatus,
    #[serde(default)]
    pub methods: MethodSet,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<StructuredReason>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub experimental: bool,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
pub enum CapabilityStatus {
    #[serde(rename = "supported")]
    Supported,
    #[serde(rename = "degraded")]
    Degraded,
    #[serde(rename = "unsupported")]
    Unsupported,
    #[serde(rename = "experimental")]
    Experimental,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct MethodSet {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_requests: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub server_requests: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notifications: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct StructuredReason {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
}


fn timestamp_now() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let (year, month, day, hour, min, sec) = rfc3339_parts(secs as i64);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, min, sec
    )
}

fn rfc3339_parts(unix_secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = if unix_secs >= 0 {
        unix_secs / 86400
    } else {
        (unix_secs - 86399) / 86400
    };
    let remaining = unix_secs - days * 86400;
    let hour = (remaining / 3600) as u32;
    let min = ((remaining % 3600) / 60) as u32;
    let sec = (remaining % 60) as u32;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32, hour, min, sec)
}

pub fn build_manifest() -> CapabilityManifest {
    CapabilityManifest {
        schema_version: "1.0.0".to_string(),
        generated_at: timestamp_now(),
        server: ServerIdentity {
            protocol_version: "2.0.0".to_string(),
            build_version: env!("CARGO_PKG_VERSION").to_string(),
            commit: option_env!("VERGEN_GIT_SHA")
                .unwrap_or("unknown")
                .to_string(),
            target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        },
        compatibility: CompatibilityRange {
            minimum_client_protocol: "1.0.0".to_string(),
            maximum_client_protocol: "2.0.0".to_string(),
        },
        capabilities: alpha_capabilities(),
    }
}

fn alpha_capabilities() -> Vec<CapabilityDeclaration> {
    vec![
        CapabilityDeclaration {
            id: "protocol.initialize".into(),
            version: "1.0.0".into(),
            status: CapabilityStatus::Supported,
            methods: MethodSet { client_requests: vec!["initialize".into()], notifications: vec!["initialized".into()], ..Default::default() },
            limits: None, reason: None, experimental: false,
        },
        CapabilityDeclaration {
            id: "thread.lifecycle".into(),
            version: "1.0.0".into(),
            status: CapabilityStatus::Supported,
            methods: MethodSet {
                client_requests: vec!["thread/start".into(), "thread/read".into(), "thread/resume".into(), "thread/list".into(), "thread/archive".into()],
                notifications: vec!["thread/started".into(), "thread/status/changed".into(), "thread/archived".into()],
                ..Default::default()
            },
            limits: Some(HashMap::from([("maxConcurrentThreads".into(), serde_json::json!(16))])),
            reason: None, experimental: false,
        },
        CapabilityDeclaration {
            id: "turn.lifecycle".into(),
            version: "1.0.0".into(),
            status: CapabilityStatus::Supported,
            methods: MethodSet {
                client_requests: vec!["turn/start".into(), "turn/steer".into(), "turn/interrupt".into()],
                notifications: vec!["turn/started".into(), "turn/completed".into()],
                ..Default::default()
            },
            limits: None, reason: None, experimental: false,
        },
        CapabilityDeclaration {
            id: "approval.lifecycle".into(),
            version: "1.0.0".into(),
            status: CapabilityStatus::Supported,
            methods: MethodSet {
                server_requests: vec!["item/commandExecution/requestApproval".into(), "item/fileChange/requestApproval".into(), "item/permissions/requestApproval".into(), "item/tool/requestUserInput".into()],
                ..Default::default()
            },
            limits: None, reason: None, experimental: false,
        },
        CapabilityDeclaration {
            id: "profile.multi_workspace".into(),
            version: "1.0.0".into(),
            status: CapabilityStatus::Supported,
            methods: MethodSet {
                client_requests: vec!["thread/start".into(), "thread/list".into()],
                ..Default::default()
            },
            limits: Some(HashMap::from([("maxWorkspacesPerProfile".into(), serde_json::json!(8))])),
            reason: None, experimental: false,
        },
        CapabilityDeclaration {
            id: "memory.lifecycle".into(),
            version: "1.0.0".into(),
            status: CapabilityStatus::Unsupported,
            methods: MethodSet::default(), limits: None,
            reason: Some(StructuredReason {
                code: "memory.bridge.missing".into(),
                message: "Memory status, export and reset are unavailable.".into(),
                remediation: Some("Install a build that implements CR-104 through CR-108.".into()),
            }),
            experimental: false,
        },
        CapabilityDeclaration {
            id: "agents.crud".into(),
            version: "1.0.0".into(), status: CapabilityStatus::Unsupported,
            methods: MethodSet::default(), limits: None,
            reason: Some(StructuredReason {
                code: "agents.bridge.missing".into(), message: "Native Agent CRUD is unavailable.".into(), remediation: None,
            }),
            experimental: false,
        },
        CapabilityDeclaration {
            id: "agents.multi_agent".into(),
            version: "1.0.0".into(), status: CapabilityStatus::Experimental,
            methods: MethodSet {
                notifications: vec!["item/started".into(), "item/completed".into(), "thread/started".into()],
                ..Default::default()
            },
            limits: Some(HashMap::from([
                ("maxAgentThreads".into(), serde_json::json!(8)),
                ("maxAgentDepth".into(), serde_json::json!(3)),
            ])),
            reason: None, experimental: true,
        },
        CapabilityDeclaration {
            id: "skills.crud".into(),
            version: "1.0.0".into(), status: CapabilityStatus::Degraded,
            methods: MethodSet { client_requests: vec!["skills/list".into()], ..Default::default() },
            limits: None,
            reason: Some(StructuredReason { code: "skills.write.missing".into(), message: "Skill listing is available but write operations are not.".into(), remediation: None }),
            experimental: false,
        },
        CapabilityDeclaration {
            id: "skills.validation".into(),
            version: "1.0.0".into(), status: CapabilityStatus::Unsupported,
            methods: MethodSet::default(), limits: None,
            reason: Some(StructuredReason { code: "skills.validation.missing".into(), message: "Native Skill validation is unavailable.".into(), remediation: None }),
            experimental: false,
        },
        CapabilityDeclaration {
            id: "skills.test".into(),
            version: "1.0.0".into(), status: CapabilityStatus::Unsupported,
            methods: MethodSet::default(), limits: None,
            reason: Some(StructuredReason { code: "skills.test.missing".into(), message: "The isolated Skill test hook is unavailable.".into(), remediation: None }),
            experimental: false,
        },
        CapabilityDeclaration {
            id: "plugins.lifecycle".into(),
            version: "1.0.0".into(), status: CapabilityStatus::Unsupported,
            methods: MethodSet::default(), limits: None,
            reason: Some(StructuredReason { code: "plugins.bridge.missing".into(), message: "Plugin lifecycle operations are unavailable.".into(), remediation: None }),
            experimental: false,
        },
        CapabilityDeclaration {
            id: "plugins.permissions".into(),
            version: "1.0.0".into(), status: CapabilityStatus::Unsupported,
            methods: MethodSet::default(), limits: None,
            reason: Some(StructuredReason { code: "plugins.permissions.missing".into(), message: "Plugin permission metadata is unavailable.".into(), remediation: None }),
            experimental: false,
        },
        CapabilityDeclaration {
            id: "mcp.config".into(),
            version: "1.0.0".into(), status: CapabilityStatus::Degraded,
            methods: MethodSet { client_requests: vec!["mcpServerStatus/list".into()], ..Default::default() },
            limits: Some(HashMap::from([("maxMcpServers".into(), serde_json::json!(16))])),
            reason: Some(StructuredReason { code: "mcp.config.read_only".into(), message: "MCP status is available but configuration and reload are not.".into(), remediation: None }),
            experimental: false,
        },
        CapabilityDeclaration {
            id: "mcp.oauth".into(),
            version: "1.0.0".into(), status: CapabilityStatus::Unsupported,
            methods: MethodSet::default(), limits: None,
            reason: Some(StructuredReason { code: "mcp.oauth.missing".into(), message: "MCP OAuth is unavailable.".into(), remediation: None }),
            experimental: false,
        },
        CapabilityDeclaration {
            id: "mcp.elicitation".into(),
            version: "1.0.0".into(), status: CapabilityStatus::Unsupported,
            methods: MethodSet::default(), limits: None,
            reason: Some(StructuredReason { code: "mcp.elicitation.missing".into(), message: "MCP elicitation is unavailable.".into(), remediation: None }),
            experimental: false,
        },
        CapabilityDeclaration {
            id: "tools.discovery".into(),
            version: "1.0.0".into(), status: CapabilityStatus::Unsupported,
            methods: MethodSet::default(), limits: None,
            reason: Some(StructuredReason { code: "tools.discovery.missing".into(), message: "Managed tool discovery metadata is unavailable.".into(), remediation: None }),
            experimental: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_valid_json() {
        let manifest = build_manifest();
        let json = serde_json::to_value(&manifest).expect("serialize manifest");
        assert_eq!(json["schemaVersion"], "1.0.0");
        assert!(json["generatedAt"].is_string());
        assert_eq!(json["server"]["protocolVersion"], "2.0.0");
        assert!(json["capabilities"].is_array());
        assert!(!json["capabilities"].as_array().unwrap().is_empty());
    }

    #[test]
    fn manifest_has_alpha_capabilities() {
        let manifest = build_manifest();
        let ids: Vec<&str> = manifest.capabilities.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"protocol.initialize"));
        assert!(ids.contains(&"thread.lifecycle"));
        assert!(ids.contains(&"turn.lifecycle"));
        assert!(ids.contains(&"approval.lifecycle"));
    }

    #[test]
    fn supported_capabilities_have_methods() {
        let manifest = build_manifest();
        for cap in &manifest.capabilities {
            match cap.status {
                CapabilityStatus::Supported => {
                    assert!(!cap.methods.client_requests.is_empty() || !cap.methods.server_requests.is_empty() || !cap.methods.notifications.is_empty(),
                        "Supported capability '{}' must have at least one method", cap.id);
                }
                CapabilityStatus::Unsupported => {
                    assert!(cap.reason.is_some(),
                        "Unsupported capability '{}' should provide a reason", cap.id);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn timestamp_is_valid() {
        let ts = timestamp_now();
        assert_eq!(ts.len(), 20, "expected 20-char ISO 8601, got {ts}");
        assert!(ts.ends_with('Z'));
    }

    #[test]
    fn manifest_roundtrip() {
        let manifest = build_manifest();
        let json = serde_json::to_value(&manifest).expect("serialize");
        let back: CapabilityManifest = serde_json::from_value(json).expect("deserialize");
        assert_eq!(manifest, back);
    }
}
