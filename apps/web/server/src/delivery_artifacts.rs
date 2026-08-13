use jsonschema::{Draft, JSONSchema};
use open_web_codex_platform_contracts::{ArtifactFailureSummary, ArtifactState};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::delivery_contracts::{ContentVerifier, DeliveryKind, DeliveryRegistry};

const MAX_FINAL_ARTIFACT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ARTIFACT_JSON_DEPTH: usize = 32;
const MAX_ARTIFACT_JSON_NODES: usize = 100_000;
const MAX_ARTIFACT_JSON_STRING_BYTES: usize = 64 * 1024;
const MAX_ARTIFACT_MARKDOWN_LINE_BYTES: usize = 16 * 1024;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinalArtifactCandidate {
    pub schema: String,
    pub display_name: String,
    pub mime_type: String,
    pub workspace_relative_path: String,
    pub byte_size: i64,
    pub verifier_snapshot: Value,
}

pub(crate) fn final_artifact_candidate_with_registry(
    item: &Map<String, Value>,
    registry: &DeliveryRegistry,
) -> Result<Option<FinalArtifactCandidate>, &'static str> {
    let Some(contract) = registry.for_item(item) else {
        return Ok(None);
    };
    if !matches!(contract.kind, DeliveryKind::WorkspaceArtifact { .. }) {
        return Ok(None);
    }
    let status = item.get("status").and_then(Value::as_str);
    if status != Some("completed") {
        return Err("artifact_delivery_invalid");
    }
    if item.get("error").is_some_and(|error| !error.is_null()) {
        return Err("artifact_delivery_invalid");
    }
    let result = item
        .get("result")
        .and_then(Value::as_object)
        .ok_or("artifact_delivery_invalid")?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err("artifact_delivery_invalid");
    }
    let structured = item
        .get("result")
        .and_then(Value::as_object)
        .and_then(|result| result.get("structuredContent"))
        .and_then(Value::as_object)
        .ok_or("final_artifact_output_missing")?;
    if structured.len() != 2
        || !structured.contains_key("summary")
        || !structured.contains_key("artifact")
        || structured
            .get("summary")
            .and_then(Value::as_str)
            .is_none_or(|summary| summary.is_empty() || summary.len() > 2_000)
    {
        return Err("final_artifact_output_invalid");
    }
    let artifact = structured
        .get("artifact")
        .and_then(Value::as_object)
        .ok_or("final_artifact_descriptor_invalid")?;
    if artifact.len() != 5
        || !artifact.keys().all(|key| {
            matches!(
                key.as_str(),
                "schema" | "displayName" | "mimeType" | "workspaceRelativePath" | "byteSize"
            )
        })
    {
        return Err("final_artifact_descriptor_invalid");
    }
    if artifact.get("schema").and_then(Value::as_str) != Some(contract.schema.as_str())
        || artifact.get("displayName").and_then(Value::as_str)
            != Some(contract.display_name.as_str())
        || artifact.get("mimeType").and_then(Value::as_str) != Some(contract.mime_type.as_str())
    {
        return Err("final_artifact_contract_mismatch");
    }
    let relative_path = artifact
        .get("workspaceRelativePath")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty() && path.len() <= 1_024)
        .ok_or("final_artifact_path_invalid")?;
    let byte_size = artifact
        .get("byteSize")
        .and_then(Value::as_u64)
        .filter(|size| *size > 0 && *size <= MAX_FINAL_ARTIFACT_BYTES)
        .and_then(|size| i64::try_from(size).ok())
        .ok_or("final_artifact_size_invalid")?;
    Ok(Some(FinalArtifactCandidate {
        schema: contract.schema.to_string(),
        display_name: contract.display_name.to_string(),
        mime_type: contract.mime_type.to_string(),
        workspace_relative_path: relative_path.to_string(),
        byte_size,
        verifier_snapshot: match &contract.kind {
            DeliveryKind::WorkspaceArtifact { verifier } => verifier.snapshot(),
            DeliveryKind::InlineGeoJsonMapCard => return Err("artifact_delivery_invalid"),
        },
    }))
}

pub(crate) fn validate_materialized_bundle_with_registry(
    declared_schema: &str,
    declared_mime_type: &str,
    bytes: &[u8],
    registry: &DeliveryRegistry,
) -> Result<(), &'static str> {
    let contract = registry
        .workspace_by_schema_mime(declared_schema, declared_mime_type)
        .ok_or("artifact_schema_unsupported")?;
    let DeliveryKind::WorkspaceArtifact { verifier } = &contract.kind else {
        return Err("artifact_schema_unsupported");
    };
    validate_materialized_bundle_with_snapshot(
        declared_schema,
        declared_mime_type,
        bytes,
        &verifier.snapshot(),
    )
}

pub(crate) fn validate_materialized_bundle_with_snapshot(
    _declared_schema: &str,
    _declared_mime_type: &str,
    bytes: &[u8],
    verifier_snapshot: &Value,
) -> Result<(), &'static str> {
    let verifier = ContentVerifier::from_snapshot(verifier_snapshot)?;
    match verifier {
        ContentVerifier::MarkdownMarker { marker } => {
            validate_browser_safe_markdown(bytes, &marker)
        }
        ContentVerifier::JsonSchema { document } => {
            let root =
                serde_json::from_slice::<Value>(bytes).map_err(|_| "artifact_json_invalid")?;
            if !root.is_object() {
                return Err("artifact_bundle_invalid");
            }
            JSONSchema::options()
                .with_draft(Draft::Draft202012)
                .compile(&document)
                .map_err(|_| "artifact_bundle_contract_mismatch")?
                .validate(&root)
                .map_err(|_| "artifact_bundle_contract_mismatch")?;
            validate_browser_safe_json(bytes, &root)
        }
    }
}

#[cfg(test)]
fn final_artifact_candidate(
    item: &Map<String, Value>,
) -> Result<Option<FinalArtifactCandidate>, &'static str> {
    final_artifact_candidate_with_registry(
        item,
        &crate::delivery_contracts::warehouse_test_registry(),
    )
}

#[cfg(test)]
fn validate_materialized_bundle(declared_schema: &str, bytes: &[u8]) -> Result<(), &'static str> {
    let registry = crate::delivery_contracts::warehouse_test_registry();
    let mime_type = registry
        .workspace_by_schema_mime(declared_schema, "application/json")
        .map(|_| "application/json")
        .unwrap_or("text/markdown");
    validate_materialized_bundle_with_registry(declared_schema, mime_type, bytes, &registry)
}

fn validate_browser_safe_markdown(bytes: &[u8], marker: &str) -> Result<(), &'static str> {
    if bytes.len() > MAX_FINAL_ARTIFACT_BYTES as usize {
        return Err("artifact_content_unsafe");
    }
    let markdown = std::str::from_utf8(bytes).map_err(|_| "artifact_bundle_invalid")?;
    let (title, remainder) = markdown
        .split_once('\n')
        .ok_or("artifact_bundle_contract_mismatch")?;
    if !title.starts_with("# ")
        || title.len() <= 2
        || title.len() > 258
        || crate::event_projection::browser_text_contains_unsafe(title)
    {
        return Err("artifact_bundle_contract_mismatch");
    }
    let marker_prefix = format!("\n{marker}\n");
    let body = remainder
        .strip_prefix(&marker_prefix)
        .ok_or("artifact_bundle_contract_mismatch")?;
    if body.contains('<')
        || body.contains('>')
        || body
            .lines()
            .any(|line| line.len() > MAX_ARTIFACT_MARKDOWN_LINE_BYTES)
        || markdown
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        || crate::event_projection::browser_text_contains_unsafe(markdown)
    {
        return Err("artifact_content_unsafe");
    }
    Ok(())
}

fn validate_browser_safe_json(bytes: &[u8], value: &Value) -> Result<(), &'static str> {
    if bytes.len() > MAX_FINAL_ARTIFACT_BYTES as usize {
        return Err("artifact_content_unsafe");
    }
    let mut nodes = 0;
    validate_browser_safe_value(value, 0, &mut nodes)
}

fn validate_browser_safe_value(
    value: &Value,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), &'static str> {
    *nodes = nodes.saturating_add(1);
    if *nodes > MAX_ARTIFACT_JSON_NODES || depth > MAX_ARTIFACT_JSON_DEPTH {
        return Err("artifact_content_unsafe");
    }
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if artifact_key_is_unsafe(key) {
                    return Err("artifact_content_unsafe");
                }
                validate_browser_safe_value(value, depth + 1, nodes)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_browser_safe_value(value, depth + 1, nodes)?;
            }
        }
        Value::String(value) => {
            if value.len() > MAX_ARTIFACT_JSON_STRING_BYTES
                || crate::event_projection::browser_text_contains_unsafe(value)
            {
                return Err("artifact_content_unsafe");
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

fn artifact_key_is_unsafe(key: &str) -> bool {
    if crate::event_projection::is_sensitive_key(key) {
        return true;
    }
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    normalized.contains("config")
}

pub(crate) fn artifact_delivery_failure(code: &str) -> ArtifactFailureSummary {
    let code = match code {
        "artifact_content_unsafe" => "artifact_content_unsafe",
        "artifact_projection_failed" => "artifact_projection_failed",
        "artifact_delivery_invalid"
        | "final_artifact_output_invalid"
        | "final_artifact_output_missing"
        | "final_artifact_descriptor_invalid"
        | "final_artifact_contract_mismatch"
        | "final_artifact_path_invalid"
        | "final_artifact_size_invalid" => "artifact_delivery_invalid",
        _ => "unknown",
    };
    ArtifactFailureSummary::from_persisted(code)
}

/// Build the safe browser/event projection for one durable Artifact.
///
/// The persisted failure code is never copied through. URLs are capabilities
/// and are intentionally emitted only for a ready Artifact; a failed Artifact
/// receives the same fixed allowlisted failure summary used by the REST DTO.
pub(crate) fn artifact_delivery_projection(
    artifact_id: Uuid,
    schema: &str,
    display_name: &str,
    mime_type: &str,
    expected_size: i64,
    byte_size: Option<i64>,
    persisted_state: &str,
    failure_code: Option<&str>,
) -> Result<Value, String> {
    let state = ArtifactState::from_persisted(persisted_state)
        .ok_or_else(|| "Artifact state is invalid".to_string())?;
    let mut value = json!({
        "artifactId": artifact_id,
        "schema": schema,
        "displayName": display_name,
        "mimeType": mime_type,
        "expectedSize": expected_size,
        "byteSize": byte_size,
        "state": state,
    });
    let object = value
        .as_object_mut()
        .ok_or_else(|| "Artifact projection is not an object".to_string())?;
    if state.is_ready() {
        let content_url = format!("/api/artifacts/{artifact_id}/content");
        // The live item/event projection keeps its existing exact `url`
        // contract. Typed content/download capabilities belong to the durable
        // ArtifactSummary DTO, so this event does not advertise a second URL
        // shape.
        object.insert("url".to_string(), json!(content_url));
    }
    if matches!(state, ArtifactState::Failed) {
        let failure_code =
            failure_code.ok_or_else(|| "Failed Artifact has no failure code".to_string())?;
        let failure = ArtifactFailureSummary::from_persisted(failure_code);
        object.insert("failure".to_string(), json!(failure));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{
        artifact_delivery_failure, artifact_delivery_projection, final_artifact_candidate,
        validate_browser_safe_json, validate_materialized_bundle,
    };
    use open_web_codex_platform_contracts::ArtifactFailureCode;
    use serde_json::{json, Value};
    use uuid::Uuid;

    #[test]
    fn accepts_only_exact_final_tool_contracts() {
        let item = json!({
            "server": "supply_chain",
            "tool": "render_network_comparison_map",
            "status": "completed",
            "error": null,
            "result": {"structuredContent": {
                "summary": "Created map.",
                "artifact": {
                    "schema": "network_comparison_map_bundle.v1",
                    "displayName": "Warehouse network: baseline vs selected facilities",
                    "mimeType": "application/json",
                    "workspaceRelativePath": "outputs/network-map.json",
                    "byteSize": 128
                }
            }}
        });
        let candidate = final_artifact_candidate(item.as_object().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            candidate.workspace_relative_path,
            "outputs/network-map.json"
        );

        let mut intermediate = item.clone();
        intermediate["tool"] = json!("compare_network_scenarios");
        assert_eq!(
            final_artifact_candidate(intermediate.as_object().unwrap()).unwrap(),
            None
        );
    }

    #[test]
    fn rejects_final_output_drift() {
        let base = json!({
            "server": "supply_chain",
            "tool": "publish_network_planning_report",
            "status": "completed",
            "error": null,
            "result": {"structuredContent": {
                "summary": "Created report.",
                "artifact": {
                    "schema": "network_planning_report_markdown.v1",
                    "displayName": "Warehouse network planning report",
                    "mimeType": "text/markdown",
                    "workspaceRelativePath": "deliverables/report.md",
                    "byteSize": 128
                }
            }}
        });
        let mut wrong_server = base.clone();
        wrong_server["server"] = json!("other");
        assert_eq!(
            final_artifact_candidate(wrong_server.as_object().unwrap()).unwrap(),
            None
        );
        let mut wrong_tool = base.clone();
        wrong_tool["tool"] = json!("compare_network_scenarios");
        assert_eq!(
            final_artifact_candidate(wrong_tool.as_object().unwrap()).unwrap(),
            None
        );
        for (pointer, value) in [
            (
                "/result/structuredContent/artifact/schema",
                json!("wrong.v1"),
            ),
            (
                "/result/structuredContent/artifact/displayName",
                json!("Wrong report"),
            ),
            (
                "/result/structuredContent/artifact/mimeType",
                json!("text/html"),
            ),
            (
                "/result/structuredContent/artifact/workspaceRelativePath",
                json!(""),
            ),
            ("/result/structuredContent/artifact/byteSize", json!(0)),
            (
                "/result/structuredContent/artifact/byteSize",
                json!(100 * 1024 * 1024_u64 + 1),
            ),
        ] {
            let mut drifted = base.clone();
            *drifted.pointer_mut(pointer).unwrap() = value;
            assert!(
                final_artifact_candidate(drifted.as_object().unwrap()).is_err(),
                "accepted drift at {pointer}"
            );
        }
    }

    #[test]
    fn requires_official_completed_success_item() {
        let mut item = json!({
            "server": "supply_chain",
            "tool": "publish_network_planning_report",
            "status": "completed",
            "error": null,
            "result": {"structuredContent": {
                "summary": "Created report.",
                "artifact": {
                    "schema": "network_planning_report_markdown.v1",
                    "displayName": "Warehouse network planning report",
                    "mimeType": "text/markdown",
                    "workspaceRelativePath": "deliverables/report.md",
                    "byteSize": 128
                }
            }}
        });
        for status in ["inProgress", "failed", "cancelled", "interrupted", "future"] {
            item["status"] = json!(status);
            assert_eq!(
                final_artifact_candidate(item.as_object().unwrap()),
                Err("artifact_delivery_invalid")
            );
        }
        item["status"] = json!("completed");
        item["error"] = json!({"message": "failed"});
        assert_eq!(
            final_artifact_candidate(item.as_object().unwrap()),
            Err("artifact_delivery_invalid")
        );
        item["error"] = Value::Null;
        item["result"] = json!({"isError": true, "structuredContent": {}});
        assert_eq!(
            final_artifact_candidate(item.as_object().unwrap()),
            Err("artifact_delivery_invalid")
        );
        item["result"] = Value::Null;
        assert_eq!(
            final_artifact_candidate(item.as_object().unwrap()),
            Err("artifact_delivery_invalid")
        );
    }

    #[test]
    fn rejects_generic_browser_unsafe_content_without_rewriting() {
        for value in [
            json!({"summary": "see /Users/alice/private/report.json"}),
            json!({"summary": "see C:\\Users\\alice\\private\\report.json"}),
            json!({"summary": "mcp://supply-chain/resource"}),
            json!({"summary": "file:/private/resource.json"}),
            json!({"summary": "urn:internal-resource"}),
            json!({"credential": "<credential-fragment>"}),
            json!({"refresh_token": "<credential-fragment>"}),
            json!({"client_secret": "<credential-fragment>"}),
            json!({"config_path": "/private/config.toml"}),
        ] {
            let bytes = serde_json::to_vec(&value).unwrap();
            assert_eq!(
                validate_browser_safe_json(&bytes, &value),
                Err("artifact_content_unsafe")
            );
        }
        let public = json!({"summary": "https://example.invalid/report"});
        let bytes = serde_json::to_vec(&public).unwrap();
        assert_eq!(validate_browser_safe_json(&bytes, &public), Ok(()));
    }

    #[test]
    fn maps_delivery_failures_to_fixed_allowlisted_summaries() {
        assert_eq!(
            artifact_delivery_failure("artifact_content_unsafe").code,
            ArtifactFailureCode::ArtifactContentUnsafe
        );
        let unknown = artifact_delivery_failure("provider/<credential-fragment>");
        assert_eq!(unknown.code, ArtifactFailureCode::Unknown);
        assert!(!unknown.message.contains("credential-fragment"));
    }

    #[test]
    fn validates_materialized_bundle_schema_and_kind() {
        let report_fixture = include_bytes!(
            "../../../../copilots/warehouse-network/tools/planner/contracts/fixtures/\
network_planning_report_markdown.v1.md"
        );
        let map_fixture = include_bytes!(
            "../../../../copilots/warehouse-network/tools/planner/contracts/fixtures/\
network_comparison_map_bundle.v1.json"
        );
        validate_materialized_bundle("network_planning_report_markdown.v1", report_fixture)
            .expect("complete provider report fixture must validate");
        validate_materialized_bundle(
            "network_planning_report_markdown.v1",
            "# 当前仓网评估简报\n\n<!-- network_planning_report_markdown.v1 -->\n\n## 时效覆盖\n\n- 覆盖城市数：45/50。\n".as_bytes(),
        )
        .expect("provider-owned Chinese assessment with natural ratio must validate");
        validate_materialized_bundle("network_comparison_map_bundle.v1", map_fixture)
            .expect("complete provider map fixture must validate");

        for bytes in [
            b"# \n\n<!-- network_planning_report_markdown.v1 -->\n".as_slice(),
            b"# Warehouse network planning report\n\nMissing marker\n".as_slice(),
            b"# Warehouse network planning report\n\n<!-- wrong.v1 -->\n".as_slice(),
            b"# Warehouse network planning report\n\n<!-- network_planning_report_markdown.v1 -->\n\nSee /Users/private/report.md\n".as_slice(),
        ] {
            assert_eq!(
                validate_materialized_bundle("network_planning_report_markdown.v1", bytes),
                if bytes.ends_with(b"report.md\n") {
                    Err("artifact_content_unsafe")
                } else {
                    Err("artifact_bundle_contract_mismatch")
                }
            );
        }
    }

    #[test]
    fn projects_only_safe_state_capabilities_and_failure_summary() {
        let artifact_id = Uuid::nil();
        let pending = artifact_delivery_projection(
            artifact_id,
            "network_planning_report_markdown.v1",
            "Warehouse network planning report",
            "text/markdown",
            128,
            None,
            "pending",
            None,
        )
        .unwrap();
        assert_eq!(pending["state"], "pending");
        assert!(pending.get("url").is_none());
        assert!(pending.get("failure").is_none());

        let materializing = artifact_delivery_projection(
            artifact_id,
            "network_planning_report_markdown.v1",
            "Warehouse network planning report",
            "text/markdown",
            128,
            None,
            "materializing",
            None,
        )
        .unwrap();
        assert_eq!(materializing["state"], "materializing");
        assert!(materializing.get("url").is_none());

        let ready = artifact_delivery_projection(
            artifact_id,
            "network_planning_report_markdown.v1",
            "Warehouse network planning report",
            "text/markdown",
            128,
            Some(128),
            "ready",
            None,
        )
        .unwrap();
        assert_eq!(ready["state"], "ready");
        assert_eq!(
            ready["url"],
            "/api/artifacts/00000000-0000-0000-0000-000000000000/content"
        );
        assert!(ready.get("failure").is_none());

        let failed = artifact_delivery_projection(
            artifact_id,
            "network_planning_report_markdown.v1",
            "Warehouse network planning report",
            "text/markdown",
            128,
            None,
            "failed",
            Some("provider/<credential-fragment>"),
        )
        .unwrap();
        assert_eq!(failed["state"], "failed");
        assert!(failed.get("url").is_none());
        assert_eq!(failed["failure"]["code"], "unknown");
        assert_eq!(
            failed["failure"]["message"],
            ArtifactFailureCode::Unknown.summary()
        );
        assert!(!failed.to_string().contains("credential-fragment"));
    }

    #[test]
    fn rejects_unknown_artifact_state_instead_of_downgrading_it() {
        assert!(artifact_delivery_projection(
            Uuid::nil(),
            "network_planning_report_markdown.v1",
            "Warehouse network planning report",
            "text/markdown",
            128,
            None,
            "future-state",
            None,
        )
        .is_err());
    }
}
