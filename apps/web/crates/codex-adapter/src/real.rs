use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use crate::{AdapterError, CodexAdapter, HealthStatus};

#[derive(Default)]
struct SseFrameDecoder {
    buffer: Vec<u8>,
}

impl SseFrameDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some(frame_end) = find_frame_end(&self.buffer) {
            frames.push(self.buffer.drain(..frame_end).collect());
        }
        frames
    }
}

fn find_frame_end(buffer: &[u8]) -> Option<usize> {
    let lf = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| index + 2);
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4);
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(end), None) | (None, Some(end)) => Some(end),
        (None, None) => None,
    }
}

/// Adapter that proxies JSON-RPC and events to a real (or Tauri-daemon)
/// HTTP endpoint.
pub struct RealCodexAdapter {
    base_url: String,
    client: reqwest::Client,
}

impl RealCodexAdapter {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl CodexAdapter for RealCodexAdapter {
    async fn health(&self) -> Result<HealthStatus, AdapterError> {
        let resp = self
            .client
            .get(format!("{}/api/health", self.base_url))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(AdapterError::Unreachable(format!(
                "health check returned {}",
                resp.status()
            )));
        }

        let raw: Value = resp
            .json()
            .await
            .map_err(|e| AdapterError::Rpc(format!("failed to parse health response: {e}")))?;

        Ok(HealthStatus {
            ok: raw["ok"].as_bool().unwrap_or(false),
            version: raw["version"].as_str().unwrap_or("unknown").to_string(),
            name: raw["name"].as_str().unwrap_or("codex-daemon").to_string(),
        })
    }

    async fn rpc(&self, method: &str, params: Value) -> Result<Value, AdapterError> {
        let body = serde_json::json!({
            "method": method,
            "params": params,
            "clientVersion": "server",
        });

        let resp = self
            .client
            .post(format!("{}/api/rpc", self.base_url))
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let result: Value = resp
            .json()
            .await
            .map_err(|e| AdapterError::Rpc(format!("failed to parse RPC response: {e}")))?;

        if status.is_success() {
            if let Some(error) = result.get("error") {
                let msg = error["message"]
                    .as_str()
                    .unwrap_or("unknown RPC error")
                    .to_string();
                return Err(AdapterError::Rpc(msg));
            }
            Ok(result.get("result").cloned().unwrap_or(Value::Null))
        } else {
            let msg = result["error"]["message"]
                .as_str()
                .unwrap_or(&status.to_string())
                .to_string();
            Err(AdapterError::Rpc(msg))
        }
    }

    async fn subscribe_events(&self, sender: UnboundedSender<Vec<u8>>) -> Result<(), AdapterError> {
        let response = self
            .client
            .get(format!("{}/api/events", self.base_url))
            .send()
            .await?;

        let mut stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut decoder = SseFrameDecoder::default();
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        for frame in decoder.push(&bytes) {
                            if sender.send(frame).is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("events stream error: {e}");
                        break;
                    }
                }
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_frames_split_across_http_chunks() {
        let mut decoder = SseFrameDecoder::default();

        assert!(decoder.push(b"data: {\"method\":\"one\"}").is_empty());
        assert_eq!(
            decoder.push(b"\n\ndata: {\"method\":\"two\"}\n\n"),
            vec![
                b"data: {\"method\":\"one\"}\n\n".to_vec(),
                b"data: {\"method\":\"two\"}\n\n".to_vec(),
            ],
        );
    }

    #[test]
    fn decodes_crlf_frames() {
        let mut decoder = SseFrameDecoder::default();

        assert_eq!(
            decoder.push(b"data: {}\r\n\r\n"),
            vec![b"data: {}\r\n\r\n".to_vec()],
        );
    }
}
