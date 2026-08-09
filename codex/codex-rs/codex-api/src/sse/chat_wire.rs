use codex_protocol::protocol::TokenUsage;
use serde::Deserialize;

#[derive(Clone, Copy)]
pub(super) enum ChatFinishReason {
    Stop,
    ToolCalls,
}

#[derive(Default)]
pub(super) struct AccumulatedToolCall {
    pub(super) id: String,
    pub(super) function: AccumulatedToolCallFunction,
}

#[derive(Default)]
pub(super) struct AccumulatedToolCallFunction {
    pub(super) name: String,
    pub(super) arguments: String,
}

#[derive(Deserialize)]
pub(super) struct ChatCompletionChunk {
    pub(super) model: Option<String>,
    #[serde(default)]
    pub(super) choices: Vec<ChatChoice>,
    pub(super) usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
pub(super) struct ChatChoice {
    pub(super) index: usize,
    pub(super) delta: ChatDelta,
    pub(super) finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
pub(super) struct ChatDelta {
    pub(super) content: Option<String>,
    pub(super) refusal: Option<String>,
    pub(super) reasoning_content: Option<String>,
    #[serde(default)]
    pub(super) tool_calls: Vec<ChatToolCallDelta>,
}

#[derive(Deserialize)]
pub(super) struct ChatToolCallDelta {
    pub(super) index: Option<usize>,
    pub(super) id: Option<String>,
    pub(super) function: Option<ChatToolCallFunctionDelta>,
}

#[derive(Deserialize)]
pub(super) struct ChatToolCallFunctionDelta {
    pub(super) name: Option<String>,
    pub(super) arguments: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ChatUsage {
    #[serde(alias = "prompt_tokens")]
    input_tokens: i64,
    #[serde(default, alias = "prompt_tokens_details")]
    input_tokens_details: Option<ChatInputTokensDetails>,
    #[serde(default)]
    prompt_cache_hit_tokens: Option<i64>,
    #[serde(alias = "completion_tokens")]
    output_tokens: i64,
    #[serde(default, alias = "completion_tokens_details")]
    output_tokens_details: Option<ChatOutputTokensDetails>,
    #[serde(alias = "total_tokens")]
    total_tokens: i64,
}

impl From<ChatUsage> for TokenUsage {
    fn from(value: ChatUsage) -> Self {
        Self {
            input_tokens: value.input_tokens,
            cached_input_tokens: value
                .input_tokens_details
                .and_then(|details| details.cached_tokens)
                .or(value.prompt_cache_hit_tokens)
                .unwrap_or(0),
            cache_write_input_tokens: 0,
            output_tokens: value.output_tokens,
            reasoning_output_tokens: value
                .output_tokens_details
                .map(|details| details.reasoning_tokens)
                .unwrap_or(0),
            total_tokens: value.total_tokens,
            codex_rollout_budget_units: None,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct ChatInputTokensDetails {
    #[serde(default)]
    cached_tokens: Option<i64>,
}

#[derive(Deserialize)]
pub(super) struct ChatOutputTokensDetails {
    #[serde(default)]
    reasoning_tokens: i64,
}
