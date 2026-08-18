//! Typed translation between Codex's Responses-shaped runtime state and the
//! OpenAI-compatible Chat Completions wire contract.

use crate::common::ResponsesApiRequest;
use crate::error::ApiError;
use codex_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum ChatMessage {
    Text {
        role: String,
        content: String,
    },
    Assistant {
        role: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning_content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ChatToolCall>>,
    },
    ToolResult {
        role: String,
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub function: ChatToolCallFunction,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatTool {
    #[serde(rename = "type")]
    pub r#type: String,
    pub function: ChatToolFunction,
    #[serde(skip)]
    pub target: ChatToolTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatToolTarget {
    pub name: String,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatToolFunction {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "skip_false")]
    pub strict: bool,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatCompletionsApiRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ChatTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    pub parallel_tool_calls: bool,
    pub stream: bool,
    pub stream_options: ChatStreamOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ChatReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
}

#[derive(Debug, Default, Serialize, Clone, PartialEq)]
pub struct ChatStreamOptions {
    pub include_usage: bool,
}

fn skip_false(value: &bool) -> bool {
    !value
}

pub(super) fn unsupported(feature: &str) -> ApiError {
    ApiError::InvalidRequest {
        message: format!(
            "wire_api = \"chat\" does not support Responses-only {feature}; configure a Responses provider or remove that feature"
        ),
    }
}

/// Converts one Responses request into a bounded Chat Completions request.
/// Features without an exact bridge contract are rejected before a network
/// request, never silently omitted.
pub fn responses_request_to_chat_completions_request(
    request: ResponsesApiRequest,
) -> Result<ChatCompletionsApiRequest, ApiError> {
    let ResponsesApiRequest {
        model,
        instructions,
        input,
        tools,
        tool_choice,
        parallel_tool_calls,
        reasoning,
        store,
        stream,
        stream_options,
        include,
        service_tier,
        prompt_cache_key: _,
        text,
        client_metadata: _,
    } = request;
    if !stream {
        return Err(unsupported("non-streaming requests"));
    }
    if store {
        return Err(unsupported("server-side storage"));
    }
    if text.is_some() {
        return Err(unsupported("text controls"));
    }
    if stream_options.is_some() {
        return Err(unsupported("stream options"));
    }
    if include
        .iter()
        .any(|value| value != "reasoning.encrypted_content")
    {
        return Err(unsupported(
            "include values other than reasoning.encrypted_content",
        ));
    }
    if reasoning.as_ref().is_some_and(|reasoning| {
        matches!(
            reasoning.summary,
            Some(ReasoningSummaryConfig::Concise | ReasoningSummaryConfig::Detailed)
        )
    }) {
        return Err(unsupported("reasoning summaries"));
    }
    if reasoning
        .as_ref()
        .is_some_and(|reasoning| reasoning.context.is_some())
    {
        return Err(unsupported("reasoning context"));
    }

    let decoded_tools: Option<Vec<Value>> = tools
        .as_ref()
        .map(|tools| {
            serde_json::from_str(tools.as_raw_value().get()).map_err(|error| {
                ApiError::InvalidRequest {
                    message: format!("invalid Responses tool definition: {error}"),
                }
            })
        })
        .transpose()?;
    let mut tools = decoded_tools
        .map(|tools| responses_tools_to_chat_tools(&tools))
        .transpose()?
        .unwrap_or_default();
    let deferred_tools = tool_search_output_tools(&input)?;
    append_chat_tools(&mut tools, responses_tools_to_chat_tools(&deferred_tools)?)?;
    let tool_choice = chat_tool_choice(&tool_choice, !tools.is_empty())?;
    // `include`, `prompt_cache_key`, and `client_metadata` are Responses
    // metadata. The known encrypted-reasoning include is omitted as a
    // documented Chat capability reduction. Prompt caching and client metadata
    // are likewise explicit performance/telemetry capability reductions that
    // this generic Chat bridge does not retain. Chat has no summary control:
    // `auto` proceeds with the provider default and does not promise a summary,
    // while `none` is omitted. An explicit `concise` or `detailed` request is
    // rejected above instead of being silently discarded.
    let reasoning_effort = reasoning
        .and_then(|reasoning| reasoning.effort)
        .map(chat_reasoning_effort)
        .transpose()?;

    Ok(ChatCompletionsApiRequest {
        model,
        messages: responses_input_to_chat_messages(&input, &instructions)?,
        tools,
        tool_choice,
        parallel_tool_calls,
        stream: true,
        stream_options: ChatStreamOptions {
            include_usage: true,
        },
        reasoning_effort,
        service_tier,
    })
}

fn chat_tool_choice(choice: &str, has_tools: bool) -> Result<Option<String>, ApiError> {
    if !has_tools {
        return match choice {
            "auto" | "none" => Ok(None),
            unsupported_choice => Err(ApiError::InvalidRequest {
                message: format!(
                    "wire_api = \"chat\" cannot use tool_choice `{unsupported_choice}` without Chat-safe tools"
                ),
            }),
        };
    }
    match choice {
        "auto" => Ok(None),
        "none" | "required" => Ok(Some(choice.to_string())),
        unsupported_choice => Err(ApiError::InvalidRequest {
            message: format!(
                "wire_api = \"chat\" does not support tool_choice `{unsupported_choice}`"
            ),
        }),
    }
}

fn chat_reasoning_effort(effort: ReasoningEffortConfig) -> Result<ChatReasoningEffort, ApiError> {
    match effort {
        ReasoningEffortConfig::None => Ok(ChatReasoningEffort::None),
        ReasoningEffortConfig::Minimal => Ok(ChatReasoningEffort::Minimal),
        ReasoningEffortConfig::Low => Ok(ChatReasoningEffort::Low),
        ReasoningEffortConfig::Medium => Ok(ChatReasoningEffort::Medium),
        ReasoningEffortConfig::High => Ok(ChatReasoningEffort::High),
        ReasoningEffortConfig::XHigh => Ok(ChatReasoningEffort::XHigh),
        ReasoningEffortConfig::Max => Err(unsupported("reasoning effort `max`")),
        ReasoningEffortConfig::Ultra => Err(unsupported("reasoning effort `ultra`")),
        ReasoningEffortConfig::Custom(_) => Err(unsupported("custom reasoning effort")),
    }
}

pub fn responses_tools_to_chat_tools(tools: &[Value]) -> Result<Vec<ChatTool>, ApiError> {
    let mut out = Vec::with_capacity(tools.len());
    for tool in tools {
        match tool.get("type").and_then(Value::as_str) {
            Some("function") => out.push(convert_function_tool(tool, None, None)?),
            Some("namespace") => convert_namespace_tool(tool, &mut out)?,
            Some("tool_search") => out.push(convert_tool_search_tool(tool)?),
            Some(kind) => return Err(unsupported(&format!("{kind} tools"))),
            None => return Err(unsupported("untyped tools")),
        }
    }
    let mut deduplicated = Vec::with_capacity(out.len());
    for tool in out {
        if let Some(existing) = deduplicated
            .iter()
            .find(|existing: &&ChatTool| existing.function.name == tool.function.name)
        {
            if existing == &tool {
                continue;
            }
            return Err(ApiError::InvalidRequest {
                message: format!(
                    "wire_api = \"chat\" cannot encode colliding flattened tool name `{}`",
                    tool.function.name
                ),
            });
        }
        deduplicated.push(tool);
    }
    Ok(deduplicated)
}

fn append_chat_tools(tools: &mut Vec<ChatTool>, additions: Vec<ChatTool>) -> Result<(), ApiError> {
    for addition in additions {
        if let Some(existing) = tools
            .iter()
            .find(|tool| tool.function.name == addition.function.name)
        {
            if existing != &addition {
                return Err(ApiError::InvalidRequest {
                    message: format!(
                        "wire_api = \"chat\" cannot encode conflicting loaded tool `{}`",
                        addition.function.name
                    ),
                });
            }
            continue;
        }
        tools.push(addition);
    }
    Ok(())
}

fn tool_search_output_tools(input: &[ResponseItem]) -> Result<Vec<Value>, ApiError> {
    // A deferred schema is a one-Turn loading result. The Chat wire needs the
    // schema alongside the tool-search result so the immediately following
    // Chat request can call it, but an older Turn's result must not make that
    // schema directly callable again in a new Turn. Runtime stamps emitted
    // items with their Turn identity; retain the metadata-free behavior for
    // synthetic/legacy inputs that have no usable identity at all.
    let active_turn_id = input.iter().rev().find_map(ResponseItem::turn_id);
    let mut tools = Vec::new();
    for item in input {
        if active_turn_id.is_some_and(|turn_id| item.turn_id() != Some(turn_id)) {
            continue;
        }
        let ResponseItem::ToolSearchOutput {
            status,
            execution,
            tools: output_tools,
            ..
        } = item
        else {
            continue;
        };
        if status != "completed" || execution != "client" {
            return Err(unsupported("non-client completed tool_search output"));
        }
        tools.extend(output_tools.iter().cloned());
    }
    Ok(tools)
}

fn convert_tool_search_tool(tool: &Value) -> Result<ChatTool, ApiError> {
    let execution = tool
        .get("execution")
        .and_then(Value::as_str)
        .ok_or_else(|| unsupported("tool_search without execution"))?;
    if execution != "client" {
        return Err(unsupported("non-client tool_search"));
    }
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .ok_or_else(|| unsupported("tool_search without description"))?
        .to_string();
    let parameters = tool
        .get("parameters")
        .cloned()
        .ok_or_else(|| unsupported("tool_search without parameters"))?;
    Ok(ChatTool {
        r#type: "function".to_string(),
        function: ChatToolFunction {
            name: "tool_search".to_string(),
            description,
            strict: false,
            parameters,
        },
        target: ChatToolTarget {
            name: "tool_search".to_string(),
            namespace: None,
        },
    })
}

fn convert_namespace_tool(tool: &Value, out: &mut Vec<ChatTool>) -> Result<(), ApiError> {
    let namespace = tool
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| unsupported("unnamed namespace tools"))?;
    let namespace_description = tool
        .get("description")
        .and_then(Value::as_str)
        .filter(|description| !description.is_empty());
    let nested_tools = tool
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| unsupported("namespace tools without functions"))?;
    for nested_tool in nested_tools {
        if nested_tool.get("type").and_then(Value::as_str) != Some("function") {
            return Err(unsupported("non-function namespace tools"));
        }
        out.push(convert_function_tool(
            nested_tool,
            Some(namespace),
            namespace_description,
        )?);
    }
    Ok(())
}

fn convert_function_tool(
    tool: &Value,
    namespace: Option<&str>,
    namespace_description: Option<&str>,
) -> Result<ChatTool, ApiError> {
    let name = tool
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| unsupported("unnamed function tools"))?
        .to_string();
    if namespace.is_none() && name == "tool_search" {
        return Err(ApiError::InvalidRequest {
            message: "wire_api = \"chat\" reserves the unnamespaced tool_search function for native ToolSearch".to_string(),
        });
    }
    let tool_description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let description = match (namespace_description, tool_description.is_empty()) {
        (Some(namespace_description), false) => {
            format!("{namespace_description}\n\n{tool_description}")
        }
        (Some(namespace_description), true) => namespace_description.to_string(),
        (None, _) => tool_description.to_string(),
    };
    let strict = tool.get("strict").and_then(Value::as_bool).unwrap_or(false);
    let parameters = tool
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    let wire_name = namespace
        .map(|namespace| format!("{namespace}__{name}"))
        .unwrap_or_else(|| name.clone());

    Ok(ChatTool {
        r#type: "function".to_string(),
        function: ChatToolFunction {
            name: wire_name,
            description,
            strict,
            parameters,
        },
        target: ChatToolTarget {
            name,
            namespace: namespace.map(str::to_string),
        },
    })
}

#[path = "chat_translate_history.rs"]
mod chat_translate_history;

pub(crate) use chat_translate_history::responses_input_to_chat_messages;

#[cfg(test)]
#[path = "chat_translate_tests.rs"]
mod tests;
