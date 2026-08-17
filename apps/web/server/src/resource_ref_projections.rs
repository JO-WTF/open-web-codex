//! Bounded references to provider-owned MCP Resources from official Tool Items.
//!
//! This projection intentionally contains no Resource content, cache, version
//! policy, or semantic interpretation. It exists only so an authorized user can
//! explicitly select an exact previously produced reference for a later Turn.

use serde_json::{Map, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResourceRefCandidate {
    pub(crate) server: String,
    pub(crate) uri: String,
    pub(crate) resource_schema: String,
    pub(crate) display_name: String,
    pub(crate) producer_tool: String,
}

pub(crate) fn candidates(item: &Map<String, Value>) -> Vec<ResourceRefCandidate> {
    if item.get("type").and_then(Value::as_str) != Some("mcpToolCall") {
        return Vec::new();
    }
    let Some(server) = item
        .get("server")
        .and_then(Value::as_str)
        .filter(|value| valid_identifier(value))
    else {
        return Vec::new();
    };
    let Some(producer_tool) = item
        .get("tool")
        .and_then(Value::as_str)
        .filter(|value| valid_identifier(value))
    else {
        return Vec::new();
    };
    let Some(content) = item
        .get("result")
        .and_then(Value::as_object)
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    content
        .iter()
        .filter_map(|value| {
            let value = value.as_object()?;
            if value.get("type").and_then(Value::as_str) != Some("resource_link") {
                return None;
            }
            let uri = value.get("uri").and_then(Value::as_str)?;
            let resource_schema = value.get("title").and_then(Value::as_str)?;
            let display_name = value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(resource_schema);
            if !valid_resource_uri(uri)
                || !valid_identifier(resource_schema)
                || !valid_display_name(display_name)
            {
                return None;
            }
            Some(ResourceRefCandidate {
                server: server.to_string(),
                uri: uri.to_string(),
                resource_schema: resource_schema.to_string(),
                display_name: display_name.to_string(),
                producer_tool: producer_tool.to_string(),
            })
        })
        .take(64)
        .collect()
}

pub(crate) async fn register(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    profile_id: Uuid,
    workspace_id: Uuid,
    run_id: Uuid,
    producer_event_id: Uuid,
    producer_thread_id: &str,
    producer_turn_id: Option<&str>,
    producer_item_id: &str,
    candidates: &[ResourceRefCandidate],
) -> Result<(), String> {
    for (ordinal, candidate) in candidates.iter().enumerate() {
        sqlx::query(
            "INSERT INTO resource_ref_projections (
                organization_id, profile_id, workspace_id, run_id, producer_event_id,
                producer_thread_id, producer_turn_id, producer_item_id, ordinal,
                server, resource_uri, resource_schema, producer_tool, display_name
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14
             ) ON CONFLICT (producer_event_id, ordinal) DO NOTHING",
        )
        .bind(organization_id)
        .bind(profile_id)
        .bind(workspace_id)
        .bind(run_id)
        .bind(producer_event_id)
        .bind(producer_thread_id)
        .bind(producer_turn_id)
        .bind(producer_item_id)
        .bind(ordinal as i32)
        .bind(&candidate.server)
        .bind(&candidate.uri)
        .bind(&candidate.resource_schema)
        .bind(&candidate.producer_tool)
        .bind(&candidate.display_name)
        .execute(&mut **transaction)
        .await
        .map_err(|error| format!("resource ref projection insert error: {error}"))?;
    }
    Ok(())
}

pub(crate) async fn exact_authorized_refs(
    db: &PgPool,
    organization_id: Uuid,
    profile_id: Uuid,
    workspace_id: Uuid,
    selections: &[(Uuid, i32, String, String, String)],
) -> Result<Vec<Value>, sqlx::Error> {
    let mut refs = Vec::with_capacity(selections.len());
    for (event_id, ordinal, server, uri, resource_schema) in selections {
        let row = sqlx::query(
            "SELECT server, resource_uri, resource_schema
             FROM resource_ref_projections
             WHERE organization_id = $1 AND profile_id = $2 AND workspace_id = $3
               AND producer_event_id = $4 AND ordinal = $5
               AND server = $6 AND resource_uri = $7 AND resource_schema = $8",
        )
        .bind(organization_id)
        .bind(profile_id)
        .bind(workspace_id)
        .bind(event_id)
        .bind(ordinal)
        .bind(server)
        .bind(uri)
        .bind(resource_schema)
        .fetch_optional(db)
        .await?;
        let Some(row) = row else {
            return Ok(Vec::new());
        };
        refs.push(serde_json::json!({
            "type": "mcp_resource",
            "server": row.get::<String, _>("server"),
            "uri": row.get::<String, _>("resource_uri"),
            "resource_schema": row.get::<String, _>("resource_schema"),
        }));
    }
    Ok(refs)
}

fn valid_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(byte) if byte.is_ascii_lowercase())
        && value.len() <= 128
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
}

fn valid_display_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn valid_resource_uri(value: &str) -> bool {
    if value.is_empty() || value.len() > 2048 || value.chars().any(char::is_control) {
        return false;
    }
    let Some((scheme, resource)) = value.split_once("://") else {
        return false;
    };
    !resource.is_empty()
        && !matches!(scheme, "http" | "https" | "file")
        && scheme.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || (index > 0 && (byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.')))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projects_only_completed_tool_resource_links_with_exact_identity() {
        let item = json!({
            "type": "mcpToolCall",
            "server": "supply_chain_data",
            "tool": "normalize_network_input",
            "result": {"content": [{
                "type": "resource_link",
                "name": "normalized-network-input",
                "title": "normalized_network_input.v1",
                "uri": "supply-chain://resources/normalized-input"
            }]}
        });
        assert_eq!(
            candidates(item.as_object().expect("tool item")),
            vec![ResourceRefCandidate {
                server: "supply_chain_data".to_string(),
                uri: "supply-chain://resources/normalized-input".to_string(),
                resource_schema: "normalized_network_input.v1".to_string(),
                display_name: "normalized-network-input".to_string(),
                producer_tool: "normalize_network_input".to_string(),
            }]
        );
    }

    #[test]
    fn rejects_public_or_malformed_resource_links() {
        let item = json!({
            "type": "mcpToolCall",
            "server": "map_utils",
            "tool": "create_map_card",
            "result": {"content": [{
                "type": "resource_link",
                "name": "bad",
                "title": "map_card_spec.v1",
                "uri": "https://example.invalid/resource"
            }]}
        });
        assert!(candidates(item.as_object().expect("tool item")).is_empty());
    }
}
