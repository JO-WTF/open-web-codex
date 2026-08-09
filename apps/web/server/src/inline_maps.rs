use serde_json::{json, Map, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::event_projection::sanitize_value;

const MAP_SERVER: &str = "map_utils";
const MAP_TOOL: &str = "create_map_card";
const MAX_RENDERER_BYTES: usize = 256 * 1024;
const MAX_SOURCES: usize = 16;
const MAX_LAYERS: usize = 64;

#[derive(Debug, PartialEq)]
pub(crate) struct InlineMapCandidate {
    pub(crate) card_ref: String,
    pub(crate) renderer_payload: Value,
}

#[derive(Debug, PartialEq)]
pub(crate) struct InlineMapSource {
    pub(crate) thread_id: String,
    pub(crate) server: String,
    pub(crate) uri: String,
}

pub(crate) fn candidate(item: &Map<String, Value>) -> Option<InlineMapCandidate> {
    if item.get("type")?.as_str()? != "mcpToolCall"
        || item.get("server")?.as_str()? != MAP_SERVER
        || item.get("tool")?.as_str()? != MAP_TOOL
    {
        return None;
    }
    let structured = item
        .get("result")?
        .as_object()?
        .get("structuredContent")?
        .as_object()?;
    if structured.get("type")?.as_str()? != "open-web-artifact"
        || structured.get("kind")?.as_str()? != "inline-visualization.v1"
        || structured.keys().any(|key| {
            !matches!(
                key.as_str(),
                "type" | "kind" | "artifact" | "embed" | "warnings"
            )
        })
    {
        return None;
    }
    validate_warnings(structured.get("warnings"))?;
    let artifact = structured.get("artifact")?.as_object()?;
    if artifact
        .keys()
        .any(|key| !matches!(key.as_str(), "ref" | "renderer"))
    {
        return None;
    }
    let card_ref = artifact.get("ref")?.as_str()?.trim();
    if !valid_identifier(card_ref) {
        return None;
    }
    let renderer = artifact.get("renderer")?.as_object()?;
    if renderer.len() != 2
        || renderer.get("kind")?.as_str()? != "map.v3"
        || renderer
            .keys()
            .any(|key| !matches!(key.as_str(), "kind" | "payload"))
    {
        return None;
    }
    let renderer_payload = validate_renderer_payload(renderer.get("payload")?)?;
    let embed = structured.get("embed")?.as_object()?;
    if embed.len() != 2
        || embed.get("syntax")?.as_str()? != "codex-inline-vis.artifact.v1"
        || embed.get("code")?.as_str()? != format!("::codex-inline-vis{{artifact=\"{card_ref}\"}}")
    {
        return None;
    }
    Some(InlineMapCandidate {
        card_ref: card_ref.to_string(),
        renderer_payload,
    })
}

fn validate_warnings(value: Option<&Value>) -> Option<()> {
    let Some(value) = value else {
        return Some(());
    };
    let warnings = value.as_array()?;
    if warnings.len() > 64 {
        return None;
    }
    for warning in warnings {
        let warning = warning.as_object()?;
        if warning.len() < 2
            || warning.len() > 3
            || warning
                .keys()
                .any(|key| !matches!(key.as_str(), "code" | "path" | "message"))
        {
            return None;
        }
        if !matches!(
            warning.get("code")?.as_str()?,
            "ignored_extra_input" | "mapbox_style_warning"
        ) {
            return None;
        }
        bounded_text(warning.get("path")?.as_str()?, 512)?;
        if let Some(message) = warning.get("message") {
            bounded_text(message.as_str()?, 1024)?;
        }
    }
    Some(())
}

fn validate_renderer_payload(payload: &Value) -> Option<Value> {
    if serde_json::to_vec(payload).ok()?.len() > MAX_RENDERER_BYTES {
        return None;
    }
    let payload = payload.as_object()?;
    bounded_text(payload.get("title")?.as_str()?, 160)?;
    bounded_text(payload.get("intent")?.as_str()?, 80)?;
    if !matches!(
        payload.get("status")?.as_str()?,
        "loading" | "ready" | "error"
    ) {
        return None;
    }
    validate_sources(payload.get("sources")?)?;
    let mut safe_projection = Value::Object(payload.clone());
    for source in safe_projection
        .get_mut("sources")?
        .as_object_mut()?
        .values_mut()
    {
        source.as_object_mut()?.insert(
            "data".to_string(),
            json!({
                "type": "mcp_resource",
                "server": "provider",
                "uri": "https://example.invalid/resource",
                "format": "geojson"
            }),
        );
    }
    if sanitize_value(&safe_projection, "renderer") != safe_projection {
        return None;
    }
    let layers = payload.get("layers")?.as_array()?;
    if layers.is_empty() || layers.len() > MAX_LAYERS {
        return None;
    }
    Some(Value::Object(payload.clone()))
}

fn validate_sources(value: &Value) -> Option<()> {
    let sources = value.as_object()?;
    if sources.is_empty() || sources.len() > MAX_SOURCES {
        return None;
    }
    for (id, source) in sources {
        if !valid_identifier(id) {
            return None;
        }
        let source = source.as_object()?;
        if source.get("type")?.as_str()? != "geojson" {
            return None;
        }
        let data = source.get("data")?.as_object()?;
        if data.len() != 4
            || data.get("type")?.as_str()? != "mcp_resource"
            || data.get("format")?.as_str()? != "geojson"
        {
            return None;
        }
        let server = data.get("server")?.as_str()?;
        let uri = data.get("uri")?.as_str()?;
        if !valid_identifier(server) || server.starts_with("mcp__") || !valid_resource_uri(uri) {
            return None;
        }
    }
    Some(())
}

pub(crate) async fn register(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    run_id: Uuid,
    thread_id: &str,
    turn_id: &str,
    item_id: &str,
    candidate: &InlineMapCandidate,
) -> Result<(), String> {
    let inserted = sqlx::query(
        "INSERT INTO inline_map_cards (
            organization_id, run_id, producer_thread_id, producer_turn_id,
            producer_item_id, card_ref, renderer_payload
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (run_id, card_ref) DO NOTHING",
    )
    .bind(organization_id)
    .bind(run_id)
    .bind(thread_id)
    .bind(turn_id)
    .bind(item_id)
    .bind(&candidate.card_ref)
    .bind(&candidate.renderer_payload)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("inline map registration error: {error}"))?;
    if inserted.rows_affected() == 1 {
        return Ok(());
    }
    let existing = sqlx::query(
        "SELECT producer_thread_id, producer_turn_id, producer_item_id, renderer_payload
         FROM inline_map_cards WHERE run_id = $1 AND card_ref = $2",
    )
    .bind(run_id)
    .bind(&candidate.card_ref)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("inline map conflict lookup error: {error}"))?
    .ok_or_else(|| "inline map conflict could not be resolved".to_string())?;
    if existing.get::<String, _>("producer_thread_id") == thread_id
        && existing.get::<String, _>("producer_turn_id") == turn_id
        && existing.get::<String, _>("producer_item_id") == item_id
        && existing.get::<Value, _>("renderer_payload") == candidate.renderer_payload
    {
        Ok(())
    } else {
        Err(format!(
            "inline map ref {} was already registered",
            candidate.card_ref
        ))
    }
}

pub(crate) async fn resolve_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_id: Uuid,
    payload: &mut Value,
) -> Result<(), String> {
    if payload.pointer("/itemType").and_then(Value::as_str) != Some("agentMessage") {
        return Ok(());
    }
    let Some(text) = payload.pointer("/data/text").and_then(Value::as_str) else {
        return Ok(());
    };
    let refs = inline_map_refs(text);
    if refs.is_empty() {
        return Ok(());
    }
    let mut cards = Vec::new();
    for card_ref in refs {
        let row = sqlx::query(
            "SELECT renderer_payload FROM inline_map_cards
             WHERE run_id = $1 AND card_ref = $2",
        )
        .bind(run_id)
        .bind(&card_ref)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| format!("inline map resolution error: {error}"))?;
        if let Some(row) = row {
            let payload = browser_payload(run_id, &card_ref, row.get("renderer_payload"))?;
            cards.push(json!({
                "ref": card_ref,
                "renderer": {"kind": "map.v3", "payload": payload}
            }));
        }
    }
    if !cards.is_empty() {
        payload
            .pointer_mut("/data")
            .and_then(Value::as_object_mut)
            .expect("projected Agent Message data must be an object")
            .insert("inlineArtifacts".to_string(), Value::Array(cards));
    }
    Ok(())
}

pub(crate) async fn resolve(
    db: &PgPool,
    run_id: Uuid,
    text: &str,
) -> Result<Vec<Value>, sqlx::Error> {
    let mut cards = Vec::new();
    for card_ref in inline_map_refs(text) {
        let row = sqlx::query(
            "SELECT renderer_payload FROM inline_map_cards
             WHERE run_id = $1 AND card_ref = $2",
        )
        .bind(run_id)
        .bind(&card_ref)
        .fetch_optional(db)
        .await?;
        if let Some(row) = row {
            if let Ok(payload) = browser_payload(run_id, &card_ref, row.get("renderer_payload")) {
                cards.push(json!({
                    "ref": card_ref,
                    "renderer": {"kind": "map.v3", "payload": payload}
                }));
            }
        }
    }
    Ok(cards)
}

pub(crate) async fn source(
    db: &PgPool,
    organization_id: Uuid,
    run_id: Uuid,
    card_ref: &str,
    source_id: &str,
) -> Result<Option<InlineMapSource>, sqlx::Error> {
    if !valid_identifier(card_ref) || !valid_identifier(source_id) {
        return Ok(None);
    }
    let row = sqlx::query(
        "SELECT producer_thread_id, renderer_payload FROM inline_map_cards
         WHERE organization_id = $1 AND run_id = $2 AND card_ref = $3",
    )
    .bind(organization_id)
    .bind(run_id)
    .bind(card_ref)
    .fetch_optional(db)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let payload = row.get::<Value, _>("renderer_payload");
    let data = payload
        .pointer(&format!("/sources/{}/data", escape_pointer(source_id)))
        .and_then(Value::as_object);
    let Some(data) = data else {
        return Ok(None);
    };
    if data.get("type").and_then(Value::as_str) != Some("mcp_resource")
        || data.get("format").and_then(Value::as_str) != Some("geojson")
    {
        return Ok(None);
    }
    let Some(server) = data.get("server").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(uri) = data.get("uri").and_then(Value::as_str) else {
        return Ok(None);
    };
    Ok(Some(InlineMapSource {
        thread_id: row.get("producer_thread_id"),
        server: server.to_string(),
        uri: uri.to_string(),
    }))
}

fn browser_payload(run_id: Uuid, card_ref: &str, mut payload: Value) -> Result<Value, String> {
    let sources = payload
        .get_mut("sources")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "inline map sources were invalid".to_string())?;
    for (source_id, source) in sources {
        let data = source
            .get_mut("data")
            .ok_or_else(|| "inline map source omitted data".to_string())?;
        *data = json!({
            "type": "resource",
            "format": "geojson",
            "url": format!(
                "/api/runs/{run_id}/inline-maps/{card_ref}/sources/{source_id}"
            )
        });
    }
    Ok(payload)
}

fn inline_map_refs(markdown: &str) -> Vec<String> {
    const PREFIX: &str = "::codex-inline-vis{artifact=\"";
    let mut refs = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for source_line in markdown.lines() {
        let line = source_line.trim_end_matches('\r');
        let leading_spaces = line.bytes().take_while(|byte| *byte == b' ').count();
        let trimmed_start = &line[leading_spaces.min(line.len())..];
        let fence_char = trimmed_start.as_bytes().first().copied();
        if leading_spaces <= 3 && matches!(fence_char, Some(b'`' | b'~')) {
            let marker = fence_char.expect("matched fence marker") as char;
            let marker_len = trimmed_start
                .chars()
                .take_while(|value| *value == marker)
                .count();
            if marker_len >= 3 {
                match fence {
                    Some((active, minimum)) if active == marker && marker_len >= minimum => {
                        fence = None;
                    }
                    None => fence = Some((marker, marker_len)),
                    _ => {}
                }
                continue;
            }
        }
        if fence.is_some() || leading_spaces >= 4 || line.starts_with('\t') {
            continue;
        }
        let directive = line.trim();
        let Some(value) = directive
            .strip_prefix(PREFIX)
            .and_then(|value| value.strip_suffix("\"}"))
        else {
            continue;
        };
        if valid_identifier(value) && !refs.iter().any(|item| item == value) {
            refs.push(value.to_string());
        }
    }
    refs
}

fn bounded_text(value: &str, maximum: usize) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty() && value.chars().count() <= maximum && !value.chars().any(char::is_control))
        .then_some(value)
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
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

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(server: &str, tool: &str) -> Map<String, Value> {
        json!({
            "id": "item-map",
            "type": "mcpToolCall",
            "server": server,
            "tool": tool,
            "result": {"structuredContent": {
                "type": "open-web-artifact",
                "kind": "inline-visualization.v1",
                "artifact": {
                    "ref": "map-network",
                    "renderer": {"kind": "map.v3", "payload": {
                        "title": "Demand and warehouses",
                        "intent": "visualization",
                        "status": "ready",
                        "sources": {"network": {
                            "type": "geojson",
                            "data": {
                                "type": "mcp_resource",
                                "server": "supply_chain",
                                "uri": "supply-chain://resources/distribution",
                                "format": "geojson"
                            }
                        }},
                        "layers": [{
                            "id": "demand",
                            "type": "circle",
                            "source": "network",
                            "paint": {"circle-color": "#2563eb"}
                        }]
                    }}
                },
                "embed": {
                    "syntax": "codex-inline-vis.artifact.v1",
                    "code": "::codex-inline-vis{artifact=\"map-network\"}"
                }
            }}
        })
        .as_object()
        .expect("item object")
        .clone()
    }

    #[test]
    fn accepts_only_the_exact_map_card_tool_and_projects_private_sources_to_urls() {
        let mut value = Value::Object(item(MAP_SERVER, MAP_TOOL));
        value
            .pointer_mut("/result/structuredContent/artifact/renderer/payload/sources/network")
            .and_then(Value::as_object_mut)
            .expect("map source")
            .insert("lineMetrics".to_string(), Value::Bool(true));
        let map_candidate =
            candidate(value.as_object().expect("map item")).expect("valid map card");
        assert_eq!(map_candidate.card_ref, "map-network");
        let projected = browser_payload(
            Uuid::nil(),
            &map_candidate.card_ref,
            map_candidate.renderer_payload,
        )
        .expect("browser payload");
        assert_eq!(
            projected
                .pointer("/sources/network/data/type")
                .and_then(Value::as_str),
            Some("resource")
        );
        assert_eq!(
            projected.pointer("/sources/network/data/url").and_then(Value::as_str),
            Some("/api/runs/00000000-0000-0000-0000-000000000000/inline-maps/map-network/sources/network")
        );
        assert!(!projected.to_string().contains("supply-chain://"));
        assert_eq!(
            projected
                .pointer("/sources/network/lineMetrics")
                .and_then(Value::as_bool),
            Some(true)
        );

        assert!(candidate(&item("supply_chain", MAP_TOOL)).is_none());
        assert!(candidate(&item(MAP_SERVER, "get_route")).is_none());

        let mut unsafe_item = Value::Object(item(MAP_SERVER, MAP_TOOL));
        *unsafe_item
            .pointer_mut("/result/structuredContent/artifact/renderer/payload/title")
            .expect("map title") = Value::String("/private/tmp/internal-map.json".to_string());
        assert!(candidate(unsafe_item.as_object().expect("unsafe map item")).is_none());
    }

    #[test]
    fn parses_only_standalone_safe_embed_directives() {
        let markdown = "Before\n\n::codex-inline-vis{artifact=\"map-one\"}\n\n```\n::codex-inline-vis{artifact=\"map-code\"}\n```\n    ::codex-inline-vis{artifact=\"map-indent\"}\n";
        assert_eq!(inline_map_refs(markdown), vec!["map-one"]);
    }
}
