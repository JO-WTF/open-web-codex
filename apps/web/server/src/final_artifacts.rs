use serde_json::{Map, Value};

const MAX_FINAL_ARTIFACT_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FinalArtifactContract {
    pub server: &'static str,
    pub tool: &'static str,
    pub schema: &'static str,
    pub display_name: &'static str,
    pub mime_type: &'static str,
    pub bundle_kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinalArtifactCandidate {
    pub schema: String,
    pub display_name: String,
    pub mime_type: String,
    pub workspace_relative_path: String,
    pub byte_size: i64,
}

const CONTRACTS: &[FinalArtifactContract] = &[
    FinalArtifactContract {
        server: "supply_chain",
        tool: "render_network_comparison_map",
        schema: "network_comparison_map_bundle.v1",
        display_name: "Warehouse network: baseline vs selected facilities",
        mime_type: "application/json",
        bundle_kind: "network_comparison_map",
    },
    FinalArtifactContract {
        server: "supply_chain",
        tool: "publish_network_planning_report",
        schema: "network_planning_report_bundle.v1",
        display_name: "Warehouse network planning report",
        mime_type: "application/json",
        bundle_kind: "network_planning_report",
    },
];

pub(crate) fn final_artifact_candidate(
    item: &Map<String, Value>,
) -> Result<Option<FinalArtifactCandidate>, &'static str> {
    let Some(server) = item.get("server").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(tool) = item.get("tool").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(contract) = CONTRACTS
        .iter()
        .find(|contract| contract.server == server && contract.tool == tool)
    else {
        return Ok(None);
    };
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
    if artifact.get("schema").and_then(Value::as_str) != Some(contract.schema)
        || artifact.get("displayName").and_then(Value::as_str) != Some(contract.display_name)
        || artifact.get("mimeType").and_then(Value::as_str) != Some(contract.mime_type)
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
    }))
}

pub(crate) fn validate_materialized_bundle(
    declared_schema: &str,
    bytes: &[u8],
) -> Result<(), &'static str> {
    let contract = CONTRACTS
        .iter()
        .find(|contract| contract.schema == declared_schema)
        .ok_or("artifact_schema_unsupported")?;
    let root = serde_json::from_slice::<Value>(bytes)
        .map_err(|_| "artifact_json_invalid")?
        .as_object()
        .cloned()
        .ok_or("artifact_bundle_invalid")?;
    let snake_schema = root.get("schema_version");
    let camel_schema = root.get("schemaVersion");
    if snake_schema.is_some() == camel_schema.is_some()
        || snake_schema.or(camel_schema).and_then(Value::as_str) != Some(contract.schema)
        || root.get("kind").and_then(Value::as_str) != Some(contract.bundle_kind)
    {
        return Err("artifact_bundle_contract_mismatch");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{final_artifact_candidate, validate_materialized_bundle};
    use serde_json::json;

    #[test]
    fn accepts_only_exact_final_tool_contracts() {
        let item = json!({
            "server": "supply_chain",
            "tool": "render_network_comparison_map",
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
            "result": {"structuredContent": {
                "summary": "Created report.",
                "artifact": {
                    "schema": "network_planning_report_bundle.v1",
                    "displayName": "Warehouse network planning report",
                    "mimeType": "application/json",
                    "workspaceRelativePath": "deliverables/report.json",
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
    fn validates_materialized_bundle_schema_and_kind() {
        validate_materialized_bundle(
            "network_planning_report_bundle.v1",
            br#"{"schema_version":"network_planning_report_bundle.v1","kind":"network_planning_report"}"#,
        )
        .unwrap();
        validate_materialized_bundle(
            "network_comparison_map_bundle.v1",
            br#"{"schemaVersion":"network_comparison_map_bundle.v1","kind":"network_comparison_map"}"#,
        )
        .unwrap();

        for bytes in [
            br#"{"schema_version":"wrong.v1","kind":"network_planning_report"}"#.as_slice(),
            br#"{"schema_version":"network_planning_report_bundle.v1","kind":"wrong"}"#.as_slice(),
            br#"{"schema_version":"network_planning_report_bundle.v1","schemaVersion":"network_planning_report_bundle.v1","kind":"network_planning_report"}"#.as_slice(),
        ] {
            assert_eq!(
                validate_materialized_bundle("network_planning_report_bundle.v1", bytes),
                Err("artifact_bundle_contract_mismatch")
            );
        }
    }
}
