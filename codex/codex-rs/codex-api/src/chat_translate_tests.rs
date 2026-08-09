use super::*;
use crate::common::Reasoning;
use crate::common::ReasoningContext;
use crate::common::ResponsesApiTools;
use crate::common::TextControls;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
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
fn groups_raw_reasoning_assistant_text_and_tool_calls() {
    let mut request = request(None);
    request.instructions.clear();
    request.input = vec![
        ResponseItem::Reasoning {
            id: None,
            summary: Vec::new(),
            content: Some(vec![
                codex_protocol::models::ReasoningItemContent::ReasoningText {
                    text: "check route coordinates".to_string(),
                },
            ]),
            encrypted_content: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "Checking the route.".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        function_call("route", Some("mcp__maps"), "call_1"),
        ResponseItem::FunctionCallOutput {
            id: None,
            call_id: "call_1".to_string(),
            output: FunctionCallOutputPayload::from_text("12 km".to_string()),
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    assert_eq!(
        responses_request_to_chat_completions_request(request)
            .unwrap()
            .messages,
        vec![
            ChatMessage::Assistant {
                role: "assistant".to_string(),
                content: "Checking the route.".to_string(),
                reasoning_content: Some("check route coordinates".to_string()),
                tool_calls: Some(vec![ChatToolCall {
                    id: "call_1".to_string(),
                    r#type: "function".to_string(),
                    function: ChatToolCallFunction {
                        name: "mcp__maps__route".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
            },
            ChatMessage::ToolResult {
                role: "tool".to_string(),
                tool_call_id: "call_1".to_string(),
                content: "12 km".to_string(),
            },
        ]
    );
}

#[test]
fn preserves_each_supported_message_item_without_merging_or_text_rewrites() {
    let mut request = request(None);
    request.instructions.clear();
    request.input = vec![
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![
                ContentItem::InputText {
                    text: " first".to_string(),
                },
                ContentItem::InputText {
                    text: "\nsecond ".to_string(),
                },
            ],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: String::new(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: " answer ".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    assert_eq!(
        responses_request_to_chat_completions_request(request)
            .unwrap()
            .messages,
        vec![
            ChatMessage::Text {
                role: "user".to_string(),
                content: " first\nsecond ".to_string(),
            },
            ChatMessage::Text {
                role: "user".to_string(),
                content: String::new(),
            },
            ChatMessage::Assistant {
                role: "assistant".to_string(),
                content: " answer ".to_string(),
                reasoning_content: None,
                tool_calls: None,
            },
        ]
    );
}

#[test]
fn preserves_system_and_developer_sources_in_order_as_distinct_chat_messages() {
    let mut request = request(None);
    request.instructions = "top-level instructions".to_string();
    request.input = vec![
        text_message("system", "system source"),
        text_message("developer", "developer source"),
        text_message("user", "user source"),
    ];

    assert_eq!(
        responses_request_to_chat_completions_request(request)
            .unwrap()
            .messages,
        vec![
            ChatMessage::Text {
                role: "system".to_string(),
                content: "top-level instructions".to_string(),
            },
            ChatMessage::Text {
                role: "system".to_string(),
                content: "system source".to_string(),
            },
            ChatMessage::Text {
                role: "system".to_string(),
                content: "developer source".to_string(),
            },
            ChatMessage::Text {
                role: "user".to_string(),
                content: "user source".to_string(),
            },
        ]
    );
}

#[test]
fn replays_final_raw_reasoning_with_its_following_assistant_message() {
    let mut request = request(None);
    request.instructions.clear();
    request.input = vec![
        ResponseItem::Reasoning {
            id: None,
            summary: Vec::new(),
            content: Some(vec![
                codex_protocol::models::ReasoningItemContent::ReasoningText {
                    text: "inspect the route first".to_string(),
                },
            ]),
            encrypted_content: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "The route is safe.".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    assert_eq!(
        responses_request_to_chat_completions_request(request)
            .unwrap()
            .messages,
        vec![ChatMessage::Assistant {
            role: "assistant".to_string(),
            content: "The route is safe.".to_string(),
            reasoning_content: Some("inspect the route first".to_string()),
            tool_calls: None,
        }]
    );
}

#[test]
fn rejects_chat_history_items_without_typed_chat_equivalents() {
    let mut unsupported_role = request(None);
    unsupported_role.input = vec![text_message("tool", "result")];
    let mut mismatched_content = request(None);
    mismatched_content.input = vec![ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::InputText {
            text: "not assistant output".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];
    let mut agent_message = request(None);
    agent_message.input = vec![ResponseItem::AgentMessage {
        id: None,
        author: "planner".to_string(),
        recipient: "reviewer".to_string(),
        content: Vec::new(),
        internal_chat_message_metadata_passthrough: None,
    }];

    for request in [unsupported_role, mismatched_content, agent_message] {
        assert!(matches!(
            responses_request_to_chat_completions_request(request),
            Err(ApiError::InvalidRequest { .. })
        ));
    }
}

#[test]
fn rejects_non_text_or_incomplete_chat_history() {
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
    let mut incomplete_tool_group = request(None);
    incomplete_tool_group.instructions.clear();
    incomplete_tool_group.input = vec![function_call("route", None, "call_1")];

    for request in [non_text, reasoning, incomplete_tool_group] {
        assert!(matches!(
            responses_request_to_chat_completions_request(request),
            Err(ApiError::InvalidRequest { .. })
        ));
    }
}

fn function_call(name: &str, namespace: Option<&str>, call_id: &str) -> ResponseItem {
    ResponseItem::FunctionCall {
        id: None,
        name: name.to_string(),
        namespace: namespace.map(str::to_string),
        arguments: "{}".to_string(),
        encrypted_function_args: None,
        call_id: call_id.to_string(),
        internal_chat_message_metadata_passthrough: None,
    }
}
