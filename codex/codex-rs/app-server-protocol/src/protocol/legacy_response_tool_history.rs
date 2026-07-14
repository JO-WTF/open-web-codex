//! Compatibility projection for legacy rollouts that persisted raw response tool calls.
//!
//! Canonical paginated rollouts persist semantic `ItemCompleted(TurnItem)` records and do not
//! need this fallback. Older rollouts may contain only matching `FunctionCall`/`CustomToolCall`
//! response items, so this module materializes those unmatched pairs as `DynamicToolCall` items.

use std::collections::HashMap;

use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;

use crate::protocol::v2::DynamicToolCallOutputContentItem;
use crate::protocol::v2::DynamicToolCallStatus;
use crate::protocol::v2::ThreadItem;

#[derive(Clone)]
struct ResponseToolCallSnapshot {
    turn_id: Option<String>,
    namespace: Option<String>,
    tool: String,
    arguments: serde_json::Value,
}

#[derive(Default)]
pub(super) struct LegacyResponseToolHistory {
    calls: HashMap<String, ResponseToolCallSnapshot>,
}

impl LegacyResponseToolHistory {
    pub(super) fn start(
        &mut self,
        call_id: &str,
        turn_id: Option<&str>,
        namespace: Option<String>,
        tool: String,
        arguments: serde_json::Value,
    ) -> ThreadItem {
        self.calls.insert(
            call_id.to_string(),
            ResponseToolCallSnapshot {
                turn_id: turn_id.map(str::to_owned),
                namespace: namespace.clone(),
                tool: tool.clone(),
                arguments: arguments.clone(),
            },
        );
        ThreadItem::DynamicToolCall {
            id: call_id.to_string(),
            namespace,
            tool,
            arguments,
            status: DynamicToolCallStatus::InProgress,
            content_items: None,
            success: None,
            duration_ms: None,
        }
    }

    pub(super) fn complete(
        &self,
        call_id: &str,
        output_turn_id: Option<&str>,
        output: &FunctionCallOutputPayload,
    ) -> Option<(Option<String>, ThreadItem)> {
        let snapshot = self.calls.get(call_id)?.clone();
        let turn_id = output_turn_id
            .map(str::to_owned)
            .or_else(|| snapshot.turn_id.clone());
        let success = output.success.unwrap_or(true);
        let status = if success {
            DynamicToolCallStatus::Completed
        } else {
            DynamicToolCallStatus::Failed
        };
        Some((
            turn_id,
            ThreadItem::DynamicToolCall {
                id: call_id.to_string(),
                namespace: snapshot.namespace,
                tool: snapshot.tool,
                arguments: snapshot.arguments,
                status,
                content_items: Some(output_content_items(output)),
                success: Some(success),
                duration_ms: None,
            },
        ))
    }

    pub(super) fn fail_incomplete(&self, items: &mut [ThreadItem]) {
        for item in items {
            if !self.calls.contains_key(item.id()) {
                continue;
            }
            if let ThreadItem::DynamicToolCall {
                status, success, ..
            } = item
                && *status == DynamicToolCallStatus::InProgress
            {
                *status = DynamicToolCallStatus::Failed;
                *success = Some(false);
            }
        }
    }
}

fn output_content_items(
    output: &FunctionCallOutputPayload,
) -> Vec<DynamicToolCallOutputContentItem> {
    match &output.body {
        FunctionCallOutputBody::Text(text) => {
            vec![DynamicToolCallOutputContentItem::InputText { text: text.clone() }]
        }
        FunctionCallOutputBody::ContentItems(items) => items
            .iter()
            .filter_map(|item| match item {
                FunctionCallOutputContentItem::InputText { text } => {
                    Some(DynamicToolCallOutputContentItem::InputText { text: text.clone() })
                }
                FunctionCallOutputContentItem::InputImage { image_url, .. } => {
                    Some(DynamicToolCallOutputContentItem::InputImage {
                        image_url: image_url.clone(),
                    })
                }
                FunctionCallOutputContentItem::EncryptedContent { .. } => None,
            })
            .collect(),
    }
}
