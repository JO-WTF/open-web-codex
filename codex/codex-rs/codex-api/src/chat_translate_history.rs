use super::ChatMessage;
use super::ChatToolCall;
use super::ChatToolCallFunction;
use super::unsupported;
use crate::error::ApiError;
use codex_protocol::models::AgentMessageInputContent;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseItem;

pub(crate) fn responses_input_to_chat_messages(
    input: &[ResponseItem],
    instructions: &str,
) -> Result<Vec<ChatMessage>, ApiError> {
    let mut messages = Vec::with_capacity(input.len() + 1);
    let mut expected_tool_outputs = Vec::new();
    if !instructions.is_empty() {
        messages.push(ChatMessage::Text {
            role: "system".to_string(),
            content: instructions.to_string(),
        });
    }

    let mut index = 0;
    while let Some(item) = input.get(index) {
        if !expected_tool_outputs.is_empty()
            && !matches!(item, ResponseItem::FunctionCallOutput { .. })
        {
            return Err(unsupported(
                "history with a tool-call group not followed by its tool results",
            ));
        }
        match item {
            ResponseItem::Message { role, content, .. } => {
                let text = response_message_text(content)?;
                if role == "assistant" {
                    index += 1;
                    let tool_calls = take_function_calls(input, &mut index);
                    if tool_calls.is_empty() {
                        if !text.trim().is_empty() {
                            push_or_merge_text(&mut messages, "assistant".to_string(), text);
                        }
                    } else {
                        push_chat_tool_calls(
                            &mut messages,
                            text,
                            None,
                            tool_calls,
                            &mut expected_tool_outputs,
                        );
                    }
                    continue;
                }
                if !text.trim().is_empty() {
                    push_or_merge_text(&mut messages, normalize_role(role), text);
                }
            }
            ResponseItem::AgentMessage { content, .. } => {
                let text = content
                    .iter()
                    .map(|content| match content {
                        AgentMessageInputContent::InputText { text } => Ok(text.clone()),
                        AgentMessageInputContent::EncryptedContent { .. } => {
                            Err(unsupported("encrypted Agent messages"))
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join("\n");
                if !text.trim().is_empty() {
                    push_or_merge_text(&mut messages, "assistant".to_string(), text);
                }
            }
            ResponseItem::Reasoning {
                summary,
                content,
                encrypted_content,
                ..
            } => {
                let reasoning_content = raw_chat_reasoning_content(
                    summary,
                    content.as_deref(),
                    encrypted_content.as_deref(),
                )?;
                index += 1;
                let assistant_content = match input.get(index) {
                    Some(ResponseItem::Message { role, content, .. }) if role == "assistant" => {
                        index += 1;
                        response_message_text(content)?
                    }
                    _ => String::new(),
                };
                let tool_calls = take_function_calls(input, &mut index);
                if tool_calls.is_empty() {
                    if !assistant_content.trim().is_empty() {
                        push_or_merge_text(
                            &mut messages,
                            "assistant".to_string(),
                            assistant_content,
                        );
                    }
                } else {
                    push_chat_tool_calls(
                        &mut messages,
                        assistant_content,
                        Some(reasoning_content),
                        tool_calls,
                        &mut expected_tool_outputs,
                    );
                }
                continue;
            }
            ResponseItem::FunctionCall { .. } => {
                let tool_calls = take_function_calls(input, &mut index);
                push_chat_tool_calls(
                    &mut messages,
                    String::new(),
                    None,
                    tool_calls,
                    &mut expected_tool_outputs,
                );
                continue;
            }
            ResponseItem::FunctionCallOutput {
                call_id, output, ..
            } => {
                let Some(position) = expected_tool_outputs
                    .iter()
                    .position(|expected| expected == call_id)
                else {
                    return Err(unsupported(
                        "history tool result without its preceding tool-call group",
                    ));
                };
                expected_tool_outputs.remove(position);
                push_tool_result(
                    &mut messages,
                    call_id.clone(),
                    function_output_to_chat_text(output)?,
                );
            }
            ResponseItem::AdditionalTools { .. }
            | ResponseItem::LocalShellCall { .. }
            | ResponseItem::ToolSearchCall { .. }
            | ResponseItem::CustomToolCall { .. }
            | ResponseItem::CustomToolCallOutput { .. }
            | ResponseItem::ToolSearchOutput { .. }
            | ResponseItem::WebSearchCall { .. }
            | ResponseItem::ImageGenerationCall { .. }
            | ResponseItem::Compaction { .. }
            | ResponseItem::CompactionTrigger {}
            | ResponseItem::ContextCompaction { .. }
            | ResponseItem::Other => return Err(unsupported("Responses-only response items")),
        }
        index += 1;
    }
    if !expected_tool_outputs.is_empty() {
        return Err(unsupported(
            "history with a tool-call group not followed by its tool results",
        ));
    }
    Ok(messages)
}

fn response_message_text(content: &[ContentItem]) -> Result<String, ApiError> {
    content
        .iter()
        .map(content_item_to_text)
        .collect::<Result<Vec<_>, _>>()
        .map(|text| {
            text.into_iter()
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
}

fn raw_chat_reasoning_content(
    summary: &[codex_protocol::models::ReasoningItemReasoningSummary],
    content: Option<&[codex_protocol::models::ReasoningItemContent]>,
    encrypted_content: Option<&str>,
) -> Result<String, ApiError> {
    if !summary.is_empty() || encrypted_content.is_some() {
        return Err(unsupported("non-raw reasoning history"));
    }
    match content {
        Some([codex_protocol::models::ReasoningItemContent::ReasoningText { text }]) => {
            Ok(text.clone())
        }
        _ => Err(unsupported("non-raw reasoning history")),
    }
}

fn take_function_calls(input: &[ResponseItem], index: &mut usize) -> Vec<ChatToolCall> {
    let mut tool_calls = Vec::new();
    while let Some(ResponseItem::FunctionCall {
        name,
        namespace,
        arguments,
        call_id,
        ..
    }) = input.get(*index)
    {
        tool_calls.push(ChatToolCall {
            id: call_id.clone(),
            r#type: "function".to_string(),
            function: ChatToolCallFunction {
                name: namespace
                    .as_deref()
                    .map(|namespace| format!("{namespace}__{name}"))
                    .unwrap_or_else(|| name.clone()),
                arguments: arguments.clone(),
            },
        });
        *index += 1;
    }
    tool_calls
}

fn push_chat_tool_calls(
    messages: &mut Vec<ChatMessage>,
    content: String,
    reasoning_content: Option<String>,
    tool_calls: Vec<ChatToolCall>,
    expected_tool_outputs: &mut Vec<String>,
) {
    expected_tool_outputs.extend(tool_calls.iter().map(|tool_call| tool_call.id.clone()));
    messages.push(ChatMessage::AssistantWithToolCalls {
        role: "assistant".to_string(),
        content,
        reasoning_content,
        tool_calls,
    });
}

fn function_output_to_chat_text(output: &FunctionCallOutputPayload) -> Result<String, ApiError> {
    match &output.body {
        FunctionCallOutputBody::Text(text) => Ok(text.clone()),
        FunctionCallOutputBody::ContentItems(items) => items
            .iter()
            .map(|item| match item {
                FunctionCallOutputContentItem::InputText { text } => Ok(text.as_str()),
                FunctionCallOutputContentItem::InputImage { .. } => {
                    Err(unsupported("image tool output"))
                }
                FunctionCallOutputContentItem::InputAudio { .. } => {
                    Err(unsupported("audio tool output"))
                }
                FunctionCallOutputContentItem::EncryptedContent { .. } => {
                    Err(unsupported("encrypted tool output"))
                }
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|text| text.join("\n")),
    }
}

fn normalize_role(role: &str) -> String {
    match role {
        "developer" | "system" => "system".to_string(),
        "user" => "user".to_string(),
        "assistant" => "assistant".to_string(),
        other => other.to_string(),
    }
}

fn content_item_to_text(item: &ContentItem) -> Result<String, ApiError> {
    match item {
        ContentItem::InputText { text } | ContentItem::OutputText { text } => Ok(text.clone()),
        ContentItem::InputImage { .. } => Err(unsupported("image input")),
        ContentItem::InputAudio { .. } => Err(unsupported("audio input")),
    }
}

fn push_or_merge_text(messages: &mut Vec<ChatMessage>, role: String, text: String) {
    if let Some(ChatMessage::Text {
        role: last_role,
        content,
    }) = messages.last_mut()
        && *last_role == role
    {
        content.push('\n');
        content.push_str(&text);
        return;
    }
    messages.push(ChatMessage::Text {
        role,
        content: text,
    });
}

fn push_tool_result(messages: &mut Vec<ChatMessage>, tool_call_id: String, content: String) {
    let insert_at = messages
        .iter()
        .rposition(|message| matches!(message, ChatMessage::AssistantWithToolCalls { .. }))
        .map(|idx| idx + 1)
        .unwrap_or(messages.len());
    let mut insert_at = insert_at;
    while insert_at < messages.len()
        && matches!(messages[insert_at], ChatMessage::ToolResult { .. })
    {
        insert_at += 1;
    }
    messages.insert(
        insert_at,
        ChatMessage::ToolResult {
            role: "tool".to_string(),
            tool_call_id,
            content,
        },
    );
}
