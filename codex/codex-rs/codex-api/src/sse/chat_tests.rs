use super::*;
use codex_client::TransportError;
use codex_protocol::models::ReasoningItemContent;
use futures::TryStreamExt;
use pretty_assertions::assert_eq;
use tokio_test::io::Builder as IoBuilder;
use tokio_util::io::ReaderStream;

async fn collect(
    body: &str,
    targets: HashMap<String, ChatToolTarget>,
) -> Vec<Result<ResponseEvent, ApiError>> {
    let mut builder = IoBuilder::new();
    builder.read(body.as_bytes());
    let stream = ReaderStream::new(builder.build())
        .map_err(|error| TransportError::Network(error.to_string()));
    let (tx, mut rx) = mpsc::channel(16);
    tokio::spawn(process_chat_sse(
        Box::pin(stream),
        tx,
        Duration::from_secs(1),
        None,
        targets,
    ));
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    events
}

#[tokio::test]
async fn chat_sse_emits_text_usage_and_phase_neutral_message() {
    let events = collect(
        concat!(
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":3,\"completion_tokens_details\":{\"reasoning_tokens\":2},\"total_tokens\":5}}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"done\"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        ),
        HashMap::new(),
    )
    .await;
    assert!(matches!(events[1], Ok(ResponseEvent::OutputTextDelta(ref text)) if text == "done"));
    let Ok(ResponseEvent::Completed {
        token_usage: Some(token_usage),
        ..
    }) = &events[3]
    else {
        panic!("expected completed token usage");
    };
    assert_eq!(token_usage.reasoning_output_tokens, 2);
    let (added_id, phase) = match &events[0] {
        Ok(ResponseEvent::OutputItemAdded(ResponseItem::Message {
            id: Some(id),
            phase,
            ..
        })) => (id, phase),
        _ => panic!("expected assistant item start"),
    };
    let done_id = match &events[2] {
        Ok(ResponseEvent::OutputItemDone(ResponseItem::Message { id: Some(id), .. })) => id,
        _ => panic!("expected assistant item completion"),
    };
    assert_eq!(added_id, done_id);
    assert_eq!(phase, &None);
}

#[tokio::test]
async fn chat_sse_closes_reasoning_and_message_before_namespaced_tool() {
    let events = collect(
        concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"check route coordinates\"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Checking the route.\",\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"mcp__maps__route\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        ),
        HashMap::from([(
            "mcp__maps__route".to_string(),
            ChatToolTarget {
                name: "route".to_string(),
                namespace: Some("mcp__maps".to_string()),
            },
        )]),
    )
    .await;

    assert!(matches!(
        &events[0],
        Ok(ResponseEvent::OutputItemAdded(
            ResponseItem::Reasoning { .. }
        ))
    ));
    assert!(matches!(
        &events[1],
        Ok(ResponseEvent::ReasoningContentDelta { delta, content_index: 0 }) if delta == "check route coordinates"
    ));
    assert!(matches!(
        &events[2],
        Ok(ResponseEvent::OutputItemDone(ResponseItem::Reasoning {
            content: Some(content),
            ..
        })) if content == &vec![ReasoningItemContent::ReasoningText { text: "check route coordinates".to_string() }]
    ));
    assert!(matches!(
        &events[5],
        Ok(ResponseEvent::OutputItemDone(ResponseItem::Message {
            phase: None,
            ..
        }))
    ));
    assert!(matches!(
        &events[6],
        Ok(ResponseEvent::OutputItemAdded(
            ResponseItem::FunctionCall { .. }
        ))
    ));
    let Ok(ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
        name, namespace, ..
    })) = &events[7]
    else {
        panic!("expected namespaced function call completion");
    };
    assert_eq!(name, "route");
    assert_eq!(namespace.as_deref(), Some("mcp__maps"));
    assert!(matches!(&events[8], Ok(ResponseEvent::Completed { .. })));
}

#[tokio::test]
async fn chat_sse_rejects_invalid_wire_and_terminal_shapes() {
    let cases = [
        (
            "invalid-json",
            "data: {not-json}\n\n",
            "invalid chat completion SSE payload",
        ),
        ("early-close", "", "closed before [DONE]"),
        (
            "no-finish",
            concat!(
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"}}]}\n\n",
                "data: [DONE]\n\n"
            ),
            "before a supported finish_reason",
        ),
        (
            "empty-tool-calls",
            concat!(
                "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
                "data: [DONE]\n\n"
            ),
            "tool_calls` without tool calls",
        ),
        (
            "stop-with-tool",
            concat!(
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"route\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n"
            ),
            "`stop` after tool calls",
        ),
        (
            "empty-stop",
            concat!(
                "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n"
            ),
            "`stop` without assistant text",
        ),
    ];
    for (_name, body, expected) in cases {
        let events = collect(body, HashMap::new()).await;
        assert!(
            matches!(&events[events.len() - 1], Err(ApiError::Stream(message)) if message.contains(expected))
        );
    }
    for finish_reason in ["length", "content_filter", "unexpected"] {
        let events = collect(
            &format!(
                "data: {{\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"{finish_reason}\"}}]}}\n\n"
            ),
            HashMap::new(),
        )
        .await;
        assert!(
            matches!(&events[0], Err(ApiError::Stream(message)) if message.contains(finish_reason))
        );
    }
    let unknown_tool = collect(
        concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"not_offered\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        ),
        HashMap::new(),
    )
    .await;
    assert!(
        matches!(&unknown_tool[0], Err(ApiError::Stream(message)) if message.contains("unknown tool `not_offered`"))
    );
}

#[tokio::test]
async fn chat_sse_rejects_choice_refusal_and_late_reasoning() {
    let cases = [
        (
            "multiple-choice",
            "data: {\"choices\":[{\"index\":0,\"delta\":{}},{\"index\":1,\"delta\":{}}]}\n\n",
            "multiple choices",
        ),
        (
            "nonzero-choice",
            "data: {\"choices\":[{\"index\":1,\"delta\":{}}]}\n\n",
            "choice index 1",
        ),
        (
            "refusal",
            concat!(
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"}}]}\n\n",
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"refusal\":\"no\"}}]}\n\n"
            ),
            "returned a refusal",
        ),
        (
            "late-reasoning",
            concat!(
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"}}]}\n\n",
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"late\"}}]}\n\n"
            ),
            "reasoning_content after assistant output",
        ),
    ];
    for (_name, body, expected) in cases {
        let events = collect(body, HashMap::new()).await;
        assert!(
            matches!(&events[events.len() - 1], Err(ApiError::Stream(message)) if message.contains(expected))
        );
    }
}

#[tokio::test]
async fn chat_sse_uses_unique_local_ids_and_stops_on_interrupt() {
    let body = concat!(
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"done\"}}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let first = collect(body, HashMap::new()).await;
    let second = collect(body, HashMap::new()).await;
    let message_id = |events: &[Result<ResponseEvent, ApiError>]| match (&events[0], &events[2]) {
        (
            Ok(ResponseEvent::OutputItemAdded(ResponseItem::Message {
                id: Some(added), ..
            })),
            Ok(ResponseEvent::OutputItemDone(ResponseItem::Message { id: Some(done), .. })),
        ) => {
            assert_eq!(added, done);
            added.clone()
        }
        _ => panic!("expected one completed assistant message"),
    };
    assert_ne!(message_id(&first), message_id(&second));

    let mut builder = IoBuilder::new();
    builder.read(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"late\"}}]}\n\n");
    let stream = ReaderStream::new(builder.build())
        .map_err(|error| TransportError::Network(error.to_string()));
    let (tx, rx) = mpsc::channel(1);
    drop(rx);
    timeout(
        Duration::from_secs(1),
        process_chat_sse(
            Box::pin(stream),
            tx,
            Duration::from_secs(1),
            None,
            HashMap::new(),
        ),
    )
    .await
    .expect("Chat producer must stop after consumer interruption");
}
