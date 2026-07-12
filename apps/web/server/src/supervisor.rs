use sqlx::PgPool;

/// Start the RunSupervisor background task.
///
/// Listens to adapter lifecycle events (thread/completed, thread/failed)
/// via the shared EventBus and transitions run statuses in the database.
pub fn start(event_bus: tokio::sync::broadcast::Sender<Vec<u8>>, db: PgPool) {
    tokio::spawn(async move {
        let mut rx = event_bus.subscribe();

        loop {
            match rx.recv().await {
                Ok(data) => {
                    if let Err(e) = process_event(&data, &db).await {
                        tracing::warn!("supervisor event processing error: {e}");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "run supervisor lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("run supervisor: event bus closed, exiting");
                    break;
                }
            }
        }
    });
}

/// Parse an SSE frame and transition runs based on thread lifecycle events.
async fn process_event(data: &[u8], db: &PgPool) -> Result<(), String> {
    // Parse the SSE frame: "data: {json}\n\n"
    let text = std::str::from_utf8(data).map_err(|e| format!("invalid utf8: {e}"))?;
    let text = text.strip_prefix("data: ").unwrap_or(text);
    let text = text.trim();

    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("invalid json: {e}"))?;

    // Extract the inner message from app-server-event format
    // {"method":"app-server-event","params":{"workspace_id":"...","message":{...}}}
    let method = value["method"].as_str().unwrap_or("");
    if method != "app-server-event" {
        return Ok(()); // Not an event we care about
    }

    let msg = match value["params"]["message"].as_object() {
        Some(m) => m,
        None => return Ok(()),
    };

    let inner_method = msg["method"].as_str().unwrap_or("");
    match inner_method {
        "thread/completed" | "thread/failed" => {
            let status = if inner_method == "thread/completed" {
                "completed"
            } else {
                "failed"
            };

            let thread_id = match msg["params"]["threadId"].as_str() {
                Some(id) => id,
                None => {
                    tracing::warn!("supervisor: thread lifecycle event missing threadId");
                    return Ok(());
                }
            };

            let result = sqlx::query(
                "UPDATE runs SET status = $1, updated_at = now() \
                 WHERE codex_thread_id = $2 AND status = 'running'",
            )
            .bind(status)
            .bind(thread_id)
            .execute(db)
            .await
            .map_err(|e| format!("db update error: {e}"))?;

            if result.rows_affected() > 0 {
                tracing::info!(thread_id, status, "run auto-transitioned via supervisor");
            }

            Ok(())
        }
        _ => Ok(()),
    }
}
