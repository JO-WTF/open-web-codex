use std::path::PathBuf;
use std::sync::Arc;

use open_web_codex_adapter::real::RealCodexAdapter;
use open_web_codex_adapter::{AuthorizedWorkspace, CodexAdapter, TurnOptions};
use open_web_codex_profile_host::{
    ProfileHost, ProfileHostConfig, ProfileHostEvent, ProfileStartupFile,
};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Semaphore};
use tokio::time::{timeout, Duration};

const DATA_SERVER: &str = "supply_chain_data";
const DISCOVER_SOURCES: &str = "discover_workspace_sources";

#[derive(Clone)]
struct ProbeControl {
    release_root_completion: Arc<Semaphore>,
}

#[derive(Default)]
struct ProbeEvidence {
    child_thread_id: Option<String>,
    child_turn_id: Option<String>,
    root_result: Option<Value>,
    child_result: Option<Value>,
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("repository root")
        .to_path_buf()
}

fn probe_tempdir() -> tempfile::TempDir {
    let root = repository_root().join("apps/web/target/workspace-probe-tests");
    std::fs::create_dir_all(&root).expect("create test cache root");
    tempfile::Builder::new()
        .prefix("native-workspace-probe-")
        .tempdir_in(root)
        .expect("create Workspace Probe test root")
}

fn toml_string(path: &std::path::Path) -> String {
    serde_json::to_string(
        path.to_str()
            .expect("Workspace Probe paths must be valid UTF-8"),
    )
    .expect("serialize TOML-compatible string")
}

fn probe_mcp_config(
    launcher: &std::path::Path,
    asset_root: &std::path::Path,
    profile_home: &std::path::Path,
    profile_runtime: &std::path::Path,
    logs_root: &std::path::Path,
    venv: &std::path::Path,
) -> String {
    format!(
        r#"[mcp_servers.supply_chain_data]
command = {}
args = ["--data-server", "--workspace-root", {}]
cwd = {}
enabled = true
required = true
enabled_tools = ["discover_workspace_sources"]
default_tools_approval_mode = "approve"

[mcp_servers.supply_chain_data.env]
CODEX_HOME = {}
OPEN_WEB_CODEX_DATA_DIR = {}
OPEN_WEB_CODEX_LOG_DIR = {}
OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV = {}
SUPPLY_CHAIN_MCP_AUTO_INSTALL = "0"
PYTHONDONTWRITEBYTECODE = "1"
"#,
        toml_string(launcher),
        toml_string(asset_root),
        toml_string(asset_root),
        toml_string(profile_home),
        toml_string(profile_runtime),
        toml_string(logs_root),
        toml_string(venv),
    )
}

fn probe_child_role(mcp_config: &str) -> ProfileStartupFile {
    let config_toml = format!(
        r#"name = "workspace_probe_child"
description = "Calls Workspace source discovery in a native child Thread."
developer_instructions = '''
You are the child Workspace Probe. Call only the Workspace source discovery tool.
'''

[[skills.config]]
name = "workspace-probe-root"
enabled = false

[[skills.config]]
name = "workspace-probe-child"
enabled = true

{mcp_config}
"#
    );
    ProfileStartupFile::agent_role("workspace_probe_child", config_toml.into_bytes())
        .expect("valid native child Role seed")
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk).await.expect("read mock request");
        assert_ne!(read, 0, "mock Responses request ended before its body");
        request.extend_from_slice(&chunk[..read]);
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| {
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("numeric content length")
                })
            })
            .expect("Responses request content length");
        if request.len() >= header_end + 4 + content_length {
            return String::from_utf8(request).expect("UTF-8 Responses request");
        }
    }
}

fn sse(response_id: &str, item: Value) -> String {
    [
        json!({ "type": "response.created", "response": { "id": response_id } }),
        item,
        json!({
            "type": "response.completed",
            "response": {
                "id": response_id,
                "usage": {
                    "input_tokens": 0,
                    "input_tokens_details": null,
                    "output_tokens": 0,
                    "output_tokens_details": null,
                    "total_tokens": 0,
                },
            },
        }),
    ]
    .into_iter()
    .map(|event| {
        format!(
            "event: {}\ndata: {event}\n\n",
            event["type"].as_str().unwrap()
        )
    })
    .collect()
}

fn message(response_id: &str, text: &str) -> String {
    sse(
        response_id,
        json!({
            "type": "response.output_item.done",
            "item": {
                "type": "message",
                "role": "assistant",
                "id": format!("{response_id}-message"),
                "content": [{ "type": "output_text", "text": text }],
            },
        }),
    )
}

fn mcp_call(response_id: &str, call_id: &str) -> String {
    sse(
        response_id,
        json!({
            "type": "response.output_item.done",
            "item": {
                "type": "function_call",
                "call_id": call_id,
                "namespace": "mcp__supply_chain_data",
                "name": DISCOVER_SOURCES,
                "arguments": "{}",
            },
        }),
    )
}

fn spawn_call(response_id: &str, country: &str) -> String {
    sse(
        response_id,
        json!({
            "type": "response.output_item.done",
            "item": {
                "type": "function_call",
                "call_id": format!("root-{country}-spawn"),
                "namespace": "collaboration",
                "name": "spawn_agent",
                "arguments": json!({
                    "message": format!("workspace-probe-child-{country}"),
                    "task_name": format!("workspace_probe_{country}"),
                    "agent_type": "workspace_probe_child",
                    "fork_turns": "none",
                }).to_string(),
            },
        }),
    )
}

async fn response_for(request: &str, control: &ProbeControl) -> String {
    for country in ["indonesia", "thailand"] {
        let root_mcp = format!("root-{country}-mcp");
        let child_mcp = format!("child-{country}-mcp");
        let spawned = format!("root-{country}-spawn");
        if request.contains(&format!("workspace-probe-root-steer-{country}")) {
            return message(
                &format!("root-{country}-steered"),
                "Root accepted the steer.",
            );
        }
        if request.contains(&child_mcp) {
            return message(&format!("child-{country}-done"), "Child completed.");
        }
        if request.contains(&spawned) {
            control
                .release_root_completion
                .clone()
                .acquire_owned()
                .await
                .expect("Root completion control is open")
                .forget();
            return message(&format!("root-{country}-done"), "Root completed.");
        }
        if request.contains(&root_mcp) {
            return spawn_call(&format!("root-{country}-spawn"), country);
        }
        if request.contains(&format!("workspace-probe-child-{country}")) {
            return mcp_call(&format!("child-{country}-mcp"), &child_mcp);
        }
        if request.contains(&format!("workspace-probe-root-{country}")) {
            return mcp_call(&format!("root-{country}-mcp"), &root_mcp);
        }
    }
    message("unexpected", "No Workspace Probe action requested.")
}

async fn start_mock_responses_server() -> (String, ProbeControl, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind Workspace Probe Responses server");
    let address = listener
        .local_addr()
        .expect("Workspace Probe server address");
    let control = ProbeControl {
        release_root_completion: Arc::new(Semaphore::new(0)),
    };
    let server_control = control.clone();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let control = server_control.clone();
            tokio::spawn(async move {
                let request = read_request(&mut stream).await;
                let body = response_for(&request, &control).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body,
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.shutdown().await.unwrap();
            });
        }
    });
    (format!("http://{address}"), control, task)
}

fn thread_id(event: &Value) -> Option<&str> {
    event
        .pointer("/params/threadId")
        .or_else(|| event.pointer("/params/thread/id"))
        .and_then(Value::as_str)
}

async fn collect_calls(
    events: &mut broadcast::Receiver<ProfileHostEvent>,
    root_thread_id: &str,
) -> ProbeEvidence {
    let mut observed = Vec::new();
    let result = timeout(Duration::from_secs(45), async {
        let mut evidence = ProbeEvidence::default();
        loop {
            let event = events.recv().await.expect("Profile Host event").message;
            if observed.len() < 80 {
                observed.push(format!(
                    "method={} thread={:?} item={} server={} tool={}",
                    event["method"].as_str().unwrap_or("<none>"),
                    thread_id(&event),
                    event
                        .pointer("/params/item/type")
                        .and_then(Value::as_str)
                        .unwrap_or("<none>"),
                    event
                        .pointer("/params/item/server")
                        .and_then(Value::as_str)
                        .unwrap_or("<none>"),
                    event
                        .pointer("/params/item/tool")
                        .and_then(Value::as_str)
                        .unwrap_or("<none>"),
                ));
            }
            if event["method"] == "thread/started"
                && event
                    .pointer("/params/thread/parentThreadId")
                    .and_then(Value::as_str)
                    == Some(root_thread_id)
            {
                evidence.child_thread_id = thread_id(&event).map(str::to_string);
            }
            if event["method"] != "item/completed" {
                continue;
            }
            let item = &event["params"]["item"];
            if item["type"] != "mcpToolCall"
                || item["server"] != DATA_SERVER
                || item["tool"] != DISCOVER_SOURCES
            {
                continue;
            }
            if thread_id(&event) == Some(root_thread_id) {
                evidence.root_result = Some(item["result"].clone());
            } else if thread_id(&event) == evidence.child_thread_id.as_deref() {
                evidence.child_turn_id = event["params"]["turnId"].as_str().map(str::to_string);
                evidence.child_result = Some(item["result"].clone());
            }
            if evidence.root_result.is_some()
                && evidence.child_result.is_some()
                && evidence.child_thread_id.is_some()
            {
                return evidence;
            }
        }
    })
    .await;
    result.unwrap_or_else(|_| {
        panic!(
            "Root and native child did not both call the Workspace Probe MCP; observed:\n{}",
            observed.join("\n")
        )
    })
}

fn assert_sources(result: &Value, expected: &str, other: &str) {
    let result = result.to_string();
    assert!(
        result.contains(expected),
        "expected Workspace source: {result}"
    );
    assert!(
        !result.contains(other),
        "cross-Workspace source leak: {result}"
    );
    assert!(
        !result.contains("demand-cities.csv"),
        "Plugin cwd became a business Workspace: {result}"
    );
}

async fn run_probe(
    adapter: &RealCodexAdapter,
    host: &ProfileHost,
    workspace: &AuthorizedWorkspace,
    country: &str,
    expected: &str,
    other: &str,
    control: &ProbeControl,
) {
    let mut events = host.subscribe();
    let root = adapter
        .start_thread(workspace)
        .await
        .expect("start Standard Root Workspace Probe");
    let turn = adapter
        .send_user_message(
            workspace,
            &root.thread_id,
            &format!("workspace-probe-root-{country}"),
            &TurnOptions {
                access_mode: Some("read-only".to_string()),
                ..TurnOptions::default()
            },
        )
        .await
        .expect("start Root Workspace Probe Turn");
    let root_turn_id = turn["turnId"].as_str().expect("Root Turn id");
    let evidence = collect_calls(&mut events, &root.thread_id).await;
    assert_sources(evidence.root_result.as_ref().unwrap(), expected, other);
    assert_sources(evidence.child_result.as_ref().unwrap(), expected, other);
    timeout(
        Duration::from_secs(10),
        adapter.steer_turn(
            workspace,
            &root.thread_id,
            root_turn_id,
            &format!("workspace-probe-root-steer-{country}"),
            &[],
        ),
    )
    .await
    .expect("Root remains steerable while native child exists")
    .expect("Root accepts steer while native child exists");
    control.release_root_completion.add_permits(1);

    let child_thread_id = evidence.child_thread_id.unwrap();
    let child_turn_id = evidence.child_turn_id.unwrap();
    timeout(Duration::from_secs(30), async {
        let mut root_done = false;
        let mut child_done = false;
        loop {
            let event = events.recv().await.expect("Profile Host event").message;
            if event["method"] == "turn/completed" {
                let turn_id = event["params"]["turn"]["id"].as_str();
                root_done |= thread_id(&event) == Some(root.thread_id.as_str())
                    && turn_id == Some(root_turn_id);
                child_done |= thread_id(&event) == Some(child_thread_id.as_str())
                    && turn_id == Some(child_turn_id.as_str());
            }
            if root_done && child_done {
                return;
            }
        }
    })
    .await
    .expect("Root and child Workspace Probe Turns complete");
}

/// Run explicitly with:
///
/// `CODEX_BIN=/absolute/path/to/codex cargo test -p open-web-codex-adapter --test native_workspace_probe native_root_and_child_fastmcp_use_authorized_workspace -- --ignored --exact`
#[tokio::test]
#[ignore = "requires the checked-out Codex CLI and Python FastMCP dependencies"]
async fn native_root_and_child_fastmcp_use_authorized_workspace() {
    let codex_bin = PathBuf::from(std::env::var_os("CODEX_BIN").expect("CODEX_BIN is set"));
    let test_root = probe_tempdir();
    let runner_root = test_root.path().join("runner");
    let indonesia_root = runner_root.join("indonesia");
    let thailand_root = runner_root.join("thailand");
    let logs_root = test_root.path().join("logs");
    std::fs::create_dir_all(&indonesia_root).unwrap();
    std::fs::create_dir_all(&thailand_root).unwrap();
    std::fs::create_dir_all(&logs_root).unwrap();
    std::fs::write(
        indonesia_root.join("indonesia-root-child.csv"),
        "city_id,demand_quantity\nJKT,10\n",
    )
    .unwrap();
    std::fs::write(
        thailand_root.join("thailand-root-child.csv"),
        "city_id,demand_quantity\nBKK,20\n",
    )
    .unwrap();

    let (model_uri, control, model_server) = start_mock_responses_server().await;
    let profile_home = test_root.path().join("profile");
    std::fs::create_dir_all(&profile_home).unwrap();
    let profile_runtime = profile_home.join(".open-web-codex");
    let supply_chain_root = repository_root()
        .join("tools/supply-chain-network-planner")
        .canonicalize()
        .expect("canonical supply-chain application asset root");
    let supply_chain_launcher = supply_chain_root
        .join("bin/supply-chain-planner-launcher")
        .canonicalize()
        .expect("canonical supply-chain launcher");
    let supply_chain_venv = repository_root()
        .join(".local/open-web-codex/tool-envs/supply-chain-network-planner")
        .canonicalize()
        .expect("prepared supply-chain Python environment");
    let mcp_config = probe_mcp_config(
        &supply_chain_launcher,
        &supply_chain_root,
        &profile_home,
        &profile_runtime,
        &logs_root,
        &supply_chain_venv,
    );
    std::fs::write(
        profile_home.join("config.toml"),
        format!(
            r#"model = "mock-model"
model_provider = "mock_provider"
sandbox_mode = "read-only"

[features]
multi_agent_v2 = true

[skills]
include_instructions = true

[model_providers.mock_provider]
name = "Native Workspace Probe provider"
base_url = "{model_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
models = [{{ model_id = "mock-model", context_window = 25600 }}]

{mcp_config}
"#
        ),
    )
    .unwrap();
    let startup_files = [
        ProfileStartupFile::skill(
            "workspace-probe-root",
            b"---\nname: workspace-probe-root\ndescription: Root Workspace Probe\n---\nCall Workspace discovery, then spawn workspace_probe_child.\n".to_vec(),
        )
        .expect("valid Root Skill seed"),
        ProfileStartupFile::skill(
            "workspace-probe-child",
            b"---\nname: workspace-probe-child\ndescription: Child Workspace Probe\n---\nCall only discover_workspace_sources in the authorized Workspace.\n".to_vec(),
        )
        .expect("valid child Skill seed"),
        probe_child_role(&mcp_config),
    ];
    let host = ProfileHost::spawn(
        ProfileHostConfig::new("native-workspace-probe", &profile_home, &runner_root)
            .with_codex_bin(&codex_bin)
            .with_startup_files(startup_files)
            .with_environment("OPEN_WEB_CODEX_LOG_DIR", &logs_root),
    )
    .await
    .expect("spawn real Profile Host");
    let adapter =
        RealCodexAdapter::from_host(host.clone(), "workspace-probe-runner", runner_root.clone())
            .expect("construct real adapter");

    let indonesia = AuthorizedWorkspace {
        id: "indonesia".to_string(),
        root: indonesia_root.canonicalize().unwrap(),
    };
    let thailand = AuthorizedWorkspace {
        id: "thailand".to_string(),
        root: thailand_root.canonicalize().unwrap(),
    };
    run_probe(
        &adapter,
        &host,
        &indonesia,
        "indonesia",
        "indonesia-root-child.csv",
        "thailand-root-child.csv",
        &control,
    )
    .await;
    run_probe(
        &adapter,
        &host,
        &thailand,
        "thailand",
        "thailand-root-child.csv",
        "indonesia-root-child.csv",
        &control,
    )
    .await;

    for workspace in [&indonesia, &thailand] {
        assert!(
            !workspace.root.join(".codex").exists(),
            "native Profile Role/Skill/MCP activation must not write capability files into {}",
            workspace.root.display(),
        );
    }

    let launcher_log =
        std::fs::read_to_string(logs_root.join("supply-chain-network-planner-launcher.log"))
            .expect("Python FastMCP launcher log");
    assert!(launcher_log.contains(&format!("cwd={}", supply_chain_root.display())));
    assert!(
        !launcher_log.contains(&format!("cwd={}", indonesia.root.display()))
            && !launcher_log.contains(&format!("cwd={}", thailand.root.display()))
    );

    host.shutdown().await.expect("shutdown Profile Host");
    model_server.abort();
}
