use super::rpc::{build_event_notification, handle_rpc_request};
use super::*;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_HTTP_REQUEST_BYTES: usize = 1024 * 1024;

pub(super) async fn serve_web_gateway(
    listen: SocketAddr,
    token: Option<String>,
    state: Arc<DaemonState>,
    events: broadcast::Sender<DaemonEvent>,
) {
    let listener = match TcpListener::bind(listen).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("daemon web gateway: failed to bind {listen}: {err}");
            return;
        }
    };
    eprintln!("codex-monitor-daemon web gateway listening on {listen}");

    loop {
        let Ok((socket, _addr)) = listener.accept().await else {
            continue;
        };
        let state = Arc::clone(&state);
        let events = events.clone();
        let token = token.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_web_client(socket, token, state, events).await {
                eprintln!("daemon web gateway: request failed: {err}");
            }
        });
    }
}

async fn handle_web_client(
    mut socket: TcpStream,
    token: Option<String>,
    state: Arc<DaemonState>,
    events: broadcast::Sender<DaemonEvent>,
) -> Result<(), String> {
    let mut buffer = Vec::new();
    let header_end = loop {
        let mut chunk = [0_u8; 1024];
        let read = socket
            .read(&mut chunk)
            .await
            .map_err(|err| format!("read failed: {err}"))?;
        if read == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_HTTP_REQUEST_BYTES {
            write_json_response(&mut socket, 413, json!({ "error": "request too large" })).await?;
            return Ok(());
        }
        if let Some(pos) = find_header_end(&buffer) {
            break pos;
        }
    };

    let header_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let request = parse_request_headers(&header_text)?;
    let content_length = request
        .headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let mut chunk = [0_u8; 1024];
        let read = socket
            .read(&mut chunk)
            .await
            .map_err(|err| format!("read body failed: {err}"))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_HTTP_REQUEST_BYTES {
            write_json_response(&mut socket, 413, json!({ "error": "request too large" })).await?;
            return Ok(());
        }
    }
    let body = if content_length == 0 {
        Vec::new()
    } else {
        buffer[body_start..body_start + content_length.min(buffer.len().saturating_sub(body_start))]
            .to_vec()
    };

    if request.method == "OPTIONS" {
        write_empty_response(&mut socket, 204).await?;
        return Ok(());
    }

    if !is_authorized(&request, token.as_deref()) {
        write_json_response(&mut socket, 401, json!({ "error": "unauthorized" })).await?;
        return Ok(());
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/api/health") => {
            write_json_response(
                &mut socket,
                200,
                json!({
                    "ok": true,
                    "name": DAEMON_NAME,
                    "version": env!("CARGO_PKG_VERSION")
                }),
            )
            .await
        }
        ("GET", "/api/events") => stream_events(socket, events.subscribe()).await,
        ("POST", "/api/rpc") => handle_rpc_http(&mut socket, state, body).await,
        _ => write_json_response(&mut socket, 404, json!({ "error": "not found" })).await,
    }
}

async fn handle_rpc_http(
    socket: &mut TcpStream,
    state: Arc<DaemonState>,
    body: Vec<u8>,
) -> Result<(), String> {
    let payload: Value =
        serde_json::from_slice(&body).map_err(|err| format!("invalid json: {err}"))?;
    let method = payload
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing method".to_string())?;
    let params = payload.get("params").cloned().unwrap_or(Value::Null);
    let client_version = payload
        .get("clientVersion")
        .and_then(Value::as_str)
        .unwrap_or("web")
        .to_string();

    match handle_rpc_request(&state, method, params, client_version).await {
        Ok(result) => write_json_response(socket, 200, json!({ "result": result })).await,
        Err(message) => {
            write_json_response(socket, 400, json!({ "error": { "message": message } })).await
        }
    }
}

async fn stream_events(
    mut socket: TcpStream,
    mut rx: broadcast::Receiver<DaemonEvent>,
) -> Result<(), String> {
    let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
    socket
        .write_all(headers.as_bytes())
        .await
        .map_err(|err| format!("write event headers failed: {err}"))?;
    socket
        .write_all(b": connected\n\n")
        .await
        .map_err(|err| format!("write event prelude failed: {err}"))?;

    loop {
        let event = match rx.recv().await {
            Ok(event) => event,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        };
        let Some(payload) = build_event_notification(event) else {
            continue;
        };
        let frame = format!("data: {payload}\n\n");
        if socket.write_all(frame.as_bytes()).await.is_err() {
            break;
        }
    }
    Ok(())
}

struct HttpRequest {
    method: String,
    path: String,
    query: Option<String>,
    headers: HashMap<String, String>,
}

fn parse_request_headers(raw: &str) -> Result<HttpRequest, String> {
    let mut lines = raw.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default();
    if method.is_empty() || target.is_empty() {
        return Err("invalid request line".to_string());
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), Some(query.to_string())),
        None => (target.to_string(), None),
    };
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    Ok(HttpRequest {
        method,
        path,
        query,
        headers,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn is_authorized(request: &HttpRequest, expected_token: Option<&str>) -> bool {
    let Some(expected) = expected_token else {
        return true;
    };
    if let Some(header) = request.headers.get("authorization") {
        if let Some(token) = header.strip_prefix("Bearer ") {
            return token == expected;
        }
    }
    request
        .query
        .as_deref()
        .and_then(|query| {
            query.split('&').find_map(|part| {
                let (key, value) = part.split_once('=')?;
                if key == "token" {
                    Some(percent_decode(value))
                } else {
                    None
                }
            })
        })
        .is_some_and(|token| token == expected)
}

fn percent_decode(value: &str) -> String {
    let mut output = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let high = hex_value(bytes[index + 1]);
                let low = hex_value(bytes[index + 2]);
                if let (Some(high), Some(low)) = (high, low) {
                    output.push((high << 4) | low);
                    index += 3;
                } else {
                    output.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

async fn write_empty_response(socket: &mut TcpStream, status: u16) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 {status} OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: authorization, content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nContent-Length: 0\r\n\r\n"
    );
    socket
        .write_all(response.as_bytes())
        .await
        .map_err(|err| format!("write response failed: {err}"))
}

async fn write_json_response(
    socket: &mut TcpStream,
    status: u16,
    value: Value,
) -> Result<(), String> {
    let body = serde_json::to_vec(&value).map_err(|err| format!("json failed: {err}"))?;
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        413 => "Payload Too Large",
        _ => "OK",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: authorization, content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    socket
        .write_all(headers.as_bytes())
        .await
        .map_err(|err| format!("write headers failed: {err}"))?;
    socket
        .write_all(&body)
        .await
        .map_err(|err| format!("write body failed: {err}"))
}
