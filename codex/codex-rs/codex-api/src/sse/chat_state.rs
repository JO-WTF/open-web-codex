use super::chat_wire::AccumulatedToolCall;
use super::chat_wire::ChatFinishReason;
use super::chat_wire::ChatToolCallDelta;
use crate::common::ResponseEvent;
use crate::error::ApiError;
use codex_protocol::ResponseItemId;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use tokio::sync::mpsc;

pub(super) fn assistant_message_item(item_id: ResponseItemId, text: String) -> ResponseItem {
    ResponseItem::Message {
        id: Some(item_id),
        role: "assistant".to_string(),
        content: (!text.is_empty())
            .then_some(ContentItem::OutputText { text })
            .into_iter()
            .collect(),
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

pub(super) struct ChatStreamState {
    pub(super) response_id: String,
    pub(super) assistant_item_id: ResponseItemId,
    pub(super) assistant_text: String,
    pub(super) assistant_item_started: bool,
    pub(super) output_started: bool,
    reasoning_item_id: ResponseItemId,
    reasoning_content: String,
    reasoning_item_open: bool,
    pub(super) tool_calls: Vec<AccumulatedToolCall>,
    pub(super) token_usage: Option<TokenUsage>,
    pub(super) finish_reason: Option<ChatFinishReason>,
}

impl ChatStreamState {
    pub(super) fn new() -> Self {
        Self {
            response_id: ResponseItemId::new("resp").to_string(),
            assistant_item_id: ResponseItemId::new("msg"),
            assistant_text: String::new(),
            assistant_item_started: false,
            output_started: false,
            reasoning_item_id: ResponseItemId::new("rs"),
            reasoning_content: String::new(),
            reasoning_item_open: false,
            tool_calls: Vec::new(),
            token_usage: None,
            finish_reason: None,
        }
    }

    pub(super) fn assistant_message_item(&self, text: String) -> ResponseItem {
        assistant_message_item(self.assistant_item_id.clone(), text)
    }

    pub(super) async fn append_reasoning_content(
        &mut self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
        delta: String,
    ) -> Result<(), ApiError> {
        if self.output_started {
            return Err(ApiError::Stream(
                "chat completion emitted reasoning_content after assistant output began"
                    .to_string(),
            ));
        }
        if !self.reasoning_item_open {
            tx_event
                .send(Ok(ResponseEvent::OutputItemAdded(
                    self.reasoning_item_added(),
                )))
                .await
                .map_err(|_| ApiError::Stream("chat stream consumer disconnected".to_string()))?;
            self.reasoning_item_open = true;
        }
        self.reasoning_content.push_str(&delta);
        tx_event
            .send(Ok(ResponseEvent::ReasoningContentDelta {
                delta,
                content_index: 0,
            }))
            .await
            .map_err(|_| ApiError::Stream("chat stream consumer disconnected".to_string()))
    }

    pub(super) async fn finish_reasoning_item(
        &mut self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if self.reasoning_item_open {
            tx_event
                .send(Ok(ResponseEvent::OutputItemDone(
                    self.reasoning_item_done(),
                )))
                .await
                .map_err(|_| ApiError::Stream("chat stream consumer disconnected".to_string()))?;
            self.reasoning_item_open = false;
        }
        Ok(())
    }

    fn reasoning_item_added(&self) -> ResponseItem {
        ResponseItem::Reasoning {
            id: Some(self.reasoning_item_id.clone()),
            summary: Vec::new(),
            content: None,
            encrypted_content: None,
            internal_chat_message_metadata_passthrough: None,
        }
    }

    fn reasoning_item_done(&self) -> ResponseItem {
        ResponseItem::Reasoning {
            id: Some(self.reasoning_item_id.clone()),
            summary: Vec::new(),
            content: Some(vec![ReasoningItemContent::ReasoningText {
                text: self.reasoning_content.clone(),
            }]),
            encrypted_content: None,
            internal_chat_message_metadata_passthrough: None,
        }
    }

    pub(super) fn record_finish_reason(&mut self, finish_reason: &str) -> Result<(), ApiError> {
        let finish_reason = match finish_reason {
            "stop" => ChatFinishReason::Stop,
            "tool_calls" => ChatFinishReason::ToolCalls,
            "length" | "content_filter" => {
                return Err(ApiError::Stream(format!(
                    "chat completion ended with unsupported finish_reason `{finish_reason}`"
                )));
            }
            other => {
                return Err(ApiError::Stream(format!(
                    "chat completion ended with unknown finish_reason `{other}`"
                )));
            }
        };
        if self.finish_reason.replace(finish_reason).is_some() {
            return Err(ApiError::Stream(
                "chat completion emitted more than one finish_reason".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) fn merge_tool_call(&mut self, delta: ChatToolCallDelta) {
        let index = delta.index.unwrap_or(self.tool_calls.len());
        while self.tool_calls.len() <= index {
            self.tool_calls.push(AccumulatedToolCall::default());
        }
        let tool_call = &mut self.tool_calls[index];
        if let Some(id) = delta.id {
            tool_call.id = id;
        }
        if let Some(function) = delta.function {
            if let Some(name) = function.name {
                tool_call.function.name = name;
            }
            if let Some(arguments) = function.arguments {
                tool_call.function.arguments.push_str(&arguments);
            }
        }
    }
}
