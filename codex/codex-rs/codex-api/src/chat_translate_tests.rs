use super::*;
use crate::common::Reasoning;
use crate::common::ReasoningContext;
use crate::common::ResponsesApiTools;
use crate::common::TextControls;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use std::sync::Arc;

fn request(tools: Option<Vec<Value>>) -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "chat-model".to_string(),
        instructions: "be concise".to_string(),
        input: vec![text_message("user", "hello")],
        tools: tools.map(|tools| {
            ResponsesApiTools::from(Arc::from(serde_json::value::to_raw_value(&tools).unwrap()))
        }),
        tool_choice: "auto".to_string(),
        parallel_tool_calls: true,
        reasoning: None,
        store: false,
        stream: true,
        stream_options: None,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: None,
        text: None,
        client_metadata: None,
    }
}

fn text_message(role: &str, text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: role.to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

#[test]
fn translates_text_only_request_with_exact_controls() {
    let translated = responses_request_to_chat_completions_request(request(None)).unwrap();

    assert_eq!(
        translated,
        ChatCompletionsApiRequest {
            model: "chat-model".to_string(),
            messages: vec![
                ChatMessage::Text {
                    role: "system".to_string(),
                    content: "be concise".to_string(),
                },
                ChatMessage::Text {
                    role: "user".to_string(),
                    content: "hello".to_string(),
                },
            ],
            tools: Vec::new(),
            tool_choice: None,
            parallel_tool_calls: true,
            stream: true,
            stream_options: ChatStreamOptions {
                include_usage: true,
            },
            reasoning_effort: None,
            service_tier: None,
        }
    );
}

#[test]
fn maps_supported_chat_request_controls_without_approximation() {
    let tools = vec![serde_json::json!({
        "type": "function",
        "name": "route",
        "parameters": {"type": "object"}
    })];
    for (choice, expected_choice) in [
        ("auto", None),
        ("none", Some("none")),
        ("required", Some("required")),
    ] {
        let mut request = request(Some(tools.clone()));
        request.tool_choice = choice.to_string();
        request.parallel_tool_calls = false;
        request.service_tier = Some("priority".to_string());
        request.reasoning = Some(Reasoning {
            effort: Some(ReasoningEffortConfig::XHigh),
            summary: Some(ReasoningSummaryConfig::Auto),
            context: None,
        });

        let translated = responses_request_to_chat_completions_request(request).unwrap();
        assert_eq!(translated.tool_choice.as_deref(), expected_choice);
        assert!(!translated.parallel_tool_calls);
        assert_eq!(translated.service_tier.as_deref(), Some("priority"));
        assert_eq!(
            translated.reasoning_effort,
            Some(ChatReasoningEffort::XHigh)
        );
    }
}

#[test]
fn rejects_responses_controls_before_a_chat_request_is_created() {
    let mut non_streaming = request(None);
    non_streaming.stream = false;
    let mut server_store = request(None);
    server_store.store = true;
    let mut text_controls = request(None);
    text_controls.text = Some(TextControls::default());
    let mut unknown_include = request(None);
    unknown_include.include = vec!["future.include".to_string()];
    let mut reasoning_context = request(None);
    reasoning_context.reasoning = Some(Reasoning {
        effort: None,
        summary: None,
        context: Some(ReasoningContext::AllTurns),
    });

    for request in [
        non_streaming,
        server_store,
        text_controls,
        unknown_include,
        reasoning_context,
    ] {
        assert!(matches!(
            responses_request_to_chat_completions_request(request),
            Err(ApiError::InvalidRequest { .. })
        ));
    }
}

#[test]
fn preserves_only_exact_reasoning_effort_values() {
    for (effort, expected) in [
        (ReasoningEffortConfig::None, ChatReasoningEffort::None),
        (ReasoningEffortConfig::Minimal, ChatReasoningEffort::Minimal),
        (ReasoningEffortConfig::Low, ChatReasoningEffort::Low),
        (ReasoningEffortConfig::Medium, ChatReasoningEffort::Medium),
        (ReasoningEffortConfig::High, ChatReasoningEffort::High),
        (ReasoningEffortConfig::XHigh, ChatReasoningEffort::XHigh),
    ] {
        assert_eq!(chat_reasoning_effort(effort).unwrap(), expected);
    }
    for effort in [
        ReasoningEffortConfig::Max,
        ReasoningEffortConfig::Ultra,
        ReasoningEffortConfig::Custom("future".to_string()),
    ] {
        assert!(matches!(
            chat_reasoning_effort(effort),
            Err(ApiError::InvalidRequest { .. })
        ));
    }
}

#[test]
fn flattens_function_and_namespace_tools_with_reversible_targets() {
    let translated = responses_request_to_chat_completions_request(request(Some(vec![
        serde_json::json!({
            "type": "function",
            "name": "weather",
            "parameters": {"type": "object"}
        }),
        serde_json::json!({
            "type": "namespace",
            "name": "mcp__maps",
            "tools": [{
                "type": "function",
                "name": "route",
                "parameters": {"type": "object"}
            }]
        }),
    ])))
    .unwrap();

    assert_eq!(translated.tools[0].function.name, "weather");
    assert_eq!(translated.tools[1].function.name, "mcp__maps__route");
    assert_eq!(
        translated.tools[1].target,
        ChatToolTarget {
            name: "route".to_string(),
            namespace: Some("mcp__maps".to_string()),
        }
    );
}

#[test]
fn rejects_untranslated_history_instead_of_omitting_context() {
    let mut assistant = request(None);
    assistant.input = vec![ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: "previous answer".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];
    let mut non_text = request(None);
    non_text.input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: "data:image/png;base64,AA==".to_string(),
            detail: None,
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];
    let mut reasoning = request(None);
    reasoning.input = vec![ResponseItem::Reasoning {
        id: None,
        summary: Vec::new(),
        content: None,
        encrypted_content: None,
        internal_chat_message_metadata_passthrough: None,
    }];

    for request in [assistant, non_text, reasoning] {
        assert!(matches!(
            responses_request_to_chat_completions_request(request),
            Err(ApiError::InvalidRequest { .. })
        ));
    }
}
