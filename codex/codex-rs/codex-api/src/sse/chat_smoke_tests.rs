use super::process_chat_sse;
use crate::common::ResponseEvent;
use codex_client::TransportError;
use futures::TryStreamExt;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_test::io::Builder as IoBuilder;
use tokio_util::io::ReaderStream;

#[tokio::test]
async fn chat_sse_smoke_emits_one_complete_text_message() {
    let mut builder = IoBuilder::new();
    builder.read(
        concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"done\"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        )
        .as_bytes(),
    );
    let stream = ReaderStream::new(builder.build())
        .map_err(|error| TransportError::Network(error.to_string()));
    let (tx, mut rx) = mpsc::channel(8);
    tokio::spawn(process_chat_sse(
        Box::pin(stream),
        tx,
        Duration::from_secs(1),
        None,
        HashMap::new(),
    ));

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    assert!(matches!(events.as_slice(), [
        Ok(ResponseEvent::OutputItemAdded(_)),
        Ok(ResponseEvent::OutputTextDelta(text)),
        Ok(ResponseEvent::OutputItemDone(_)),
        Ok(ResponseEvent::Completed { .. }),
    ] if text == "done"));
}
