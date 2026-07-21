use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use open_web_codex_platform_contracts::{ProviderCredentialInput, UpsertProviderRequest};
use open_web_codex_profile_host::{ProfileHost, ProfileHostConfig};
use open_web_codex_provider_service::ProviderService;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn temporary_profile_home() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "open-web-codex-real-provider-service-{}-{timestamp}",
        std::process::id()
    ))
}

async fn spawn_host(codex_bin: &Path, home: &Path, workspace: &Path) -> ProfileHost {
    ProfileHost::spawn(
        ProfileHostConfig::new("provider-smoke-profile", home, workspace).with_codex_bin(codex_bin),
    )
    .await
    .expect("spawn native Profile Host")
}

async fn start_mock_models_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock model server");
    let address = listener.local_addr().expect("mock server address");
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut request = vec![0_u8; 32 * 1024];
                let count = stream.read(&mut request).await.unwrap_or_default();
                let request_line = String::from_utf8_lossy(&request[..count])
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let model_id = if request_line.contains("/provider-two/") {
                    "provider-two-model"
                } else {
                    "provider-one-model"
                };
                let body = format!(
                    r#"{{"data":[{{"id":"{model_id}","object":"model","owned_by":"smoke"}}]}}"#
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body,
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (format!("http://{address}"), task)
}

fn upsert_request(base_url: String, api_key: &str) -> UpsertProviderRequest {
    UpsertProviderRequest {
        name: "Provider smoke".to_string(),
        base_url,
        wire_api: "chat".to_string(),
        credentials: ProviderCredentialInput::Direct {
            api_key: api_key.to_string(),
        },
        select: true,
    }
}

/// Exercises Provider CRUD, provider-scoped model refresh and cache isolation
/// against the checked-out Codex app-server binary. Run with:
///
/// `CODEX_BIN=/absolute/path/to/codex cargo test -p open-web-codex-provider-service --test real_app_server -- --ignored`
#[tokio::test]
#[ignore = "requires a real Codex CLI binary"]
async fn switches_and_refreshes_isolated_provider_catalogs() {
    let codex_bin = PathBuf::from(std::env::var_os("CODEX_BIN").expect("CODEX_BIN is set"));
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("apps/web workspace root")
        .canonicalize()
        .expect("canonical workspace");
    let home = temporary_profile_home();
    std::fs::create_dir_all(&home).expect("create Profile home");
    std::fs::write(home.join("config.toml"), "").expect("write empty Profile config");
    let (models_uri, models_server) = start_mock_models_server().await;
    let host = spawn_host(&codex_bin, &home, &workspace).await;
    let service = ProviderService::for_profile_host(host.clone());

    let first_secret = "provider-one-secret-value";
    let first = service
        .upsert(
            "provider-one",
            upsert_request(format!("{models_uri}/provider-one/v1"), first_secret),
        )
        .await
        .expect("create first Provider");
    assert_eq!(first.current_provider_id, "provider-one");
    assert!(!serde_json::to_string(&first)
        .expect("serialize first catalog")
        .contains(first_secret));
    let first = service
        .refresh_models("provider-one")
        .await
        .expect("refresh first Provider");
    let first_provider = first
        .data
        .iter()
        .find(|provider| provider.id == "provider-one")
        .expect("first Provider in catalog");
    assert_eq!(first_provider.models[0].model_id, "provider-one-model");

    let second_secret = "provider-two-secret-value";
    service
        .upsert(
            "provider-two",
            upsert_request(format!("{models_uri}/provider-two/v1"), second_secret),
        )
        .await
        .expect("create second Provider");
    let second = service
        .refresh_models("provider-two")
        .await
        .expect("refresh second Provider");
    assert_eq!(second.current_provider_id, "provider-two");
    assert!(!serde_json::to_string(&second)
        .expect("serialize second catalog")
        .contains(second_secret));
    let first_provider = second
        .data
        .iter()
        .find(|provider| provider.id == "provider-one")
        .expect("first Provider remains in catalog");
    let second_provider = second
        .data
        .iter()
        .find(|provider| provider.id == "provider-two")
        .expect("second Provider in catalog");
    assert_eq!(first_provider.models[0].model_id, "provider-one-model");
    assert_eq!(second_provider.models[0].model_id, "provider-two-model");

    let selected = service
        .select("provider-one")
        .await
        .expect("select first Provider again");
    assert_eq!(selected.current_provider_id, "provider-one");
    assert_eq!(
        selected
            .data
            .iter()
            .find(|provider| provider.id == "provider-two")
            .expect("second Provider remains after switch")
            .models[0]
            .model_id,
        "provider-two-model"
    );

    host.shutdown().await.expect("shutdown Profile Host");
    drop(host);
    models_server.abort();
    std::fs::remove_dir_all(home).expect("remove smoke Profile home");
}
