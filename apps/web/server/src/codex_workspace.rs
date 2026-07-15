use std::sync::Arc;

use open_web_codex_adapter::{AdapterError, CodexAdapter};
use serde_json::{json, Value};

/// Resolve the active Codex workspace id from the adapter (first connected workspace).
pub async fn resolve_workspace_id(
    adapter: &Arc<dyn CodexAdapter>,
) -> Result<String, AdapterError> {
    let workspaces = adapter.rpc("list_workspaces", json!({})).await?;
    workspaces
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AdapterError::Internal("no workspace available".into()))
}
