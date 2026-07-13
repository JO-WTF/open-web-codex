use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use crate::{AdapterError, CodexAdapter, HealthStatus};

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

        let raw: Value = resp.json().await.map_err(|e| {
            AdapterError::Rpc(format!("failed to parse health response: {e}"))
        })?;

        Ok(HealthStatus {
            ok: raw["ok"].as_bool().unwrap_or(false),
            version: raw["version"].as_str().unwrap_or("unknown").to_string(),
            name: raw["name"]
                .as_str()
                .unwrap_or("codex-daemon")
                .to_string(),
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
        let result: Value = resp.json().await.map_err(|e| {
            AdapterError::Rpc(format!("failed to parse RPC response: {e}"))
        })?;

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

    async fn subscribe_events(
        &self,
        sender: UnboundedSender<Vec<u8>>,
    ) -> Result<(), AdapterError> {
        let response = self
            .client
            .get(format!("{}/api/events", self.base_url))
            .send()
            .await?;

        let mut stream = response.bytes_stream();

        tokio::spawn(async move {
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        if sender.send(bytes.to_vec()).is_err() {
                            break;
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
