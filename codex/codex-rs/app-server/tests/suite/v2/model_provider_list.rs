use anyhow::Result;
use app_test_support::TestAppServer;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ModelProviderKind;
use codex_app_server_protocol::ModelProviderListParams;
use codex_app_server_protocol::ModelProviderListResponse;
use tempfile::TempDir;

#[tokio::test]
async fn model_provider_list_projects_latest_config_without_credentials() -> Result<()> {
    let codex_home = TempDir::new()?;
    std::fs::write(
        codex_home.path().join("config.toml"),
        r#"
model_provider = "deepseek-e2e"

[model_providers.deepseek-e2e]
name = "DeepSeek E2E"
base_url = "https://api.deepseek.com"
experimental_bearer_token = "must-not-be-returned"
wire_api = "chat"
supports_function_tools = true

[model_providers.chat-disabled]
name = "Chat Disabled"
base_url = "https://example.invalid"
wire_api = "chat"
"#,
    )?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build_initialized()
        .await?;

    let response: ModelProviderListResponse = app
        .request(|request_id| ClientRequest::ModelProviderList {
            request_id,
            params: ModelProviderListParams {},
        })
        .await?;

    assert_eq!(response.current_provider_id, "deepseek-e2e");
    assert!(
        response
            .data
            .windows(2)
            .all(|providers| providers[0].id <= providers[1].id),
        "Provider catalog must be stable and sorted"
    );
    let deepseek = response
        .data
        .iter()
        .find(|provider| provider.id == "deepseek-e2e")
        .expect("custom Provider is listed");
    assert_eq!(deepseek.name, "DeepSeek E2E");
    assert_eq!(
        deepseek.base_url.as_deref(),
        Some("https://api.deepseek.com")
    );
    assert_eq!(deepseek.env_key, None);
    assert_eq!(deepseek.wire_api, "chat");
    assert!(deepseek.supports_function_tools);
    assert_eq!(deepseek.kind, ModelProviderKind::Custom);
    assert!(deepseek.is_current);
    assert!(deepseek.can_edit);
    assert!(!deepseek.can_delete);
    assert!(deepseek.can_fetch_models);
    assert!(deepseek.models.is_empty());
    assert_eq!(deepseek.model_count, 0);
    assert!(!serde_json::to_string(&response)?.contains("must-not-be-returned"));

    let disabled = response
        .data
        .iter()
        .find(|provider| provider.id == "chat-disabled")
        .expect("disabled custom Provider is listed");
    assert!(!disabled.supports_function_tools);

    let openai = response
        .data
        .iter()
        .find(|provider| provider.id == "openai")
        .expect("built-in OpenAI Provider is listed");
    assert_eq!(openai.kind, ModelProviderKind::BuiltIn);
    assert!(!openai.can_edit);
    assert!(!openai.can_fetch_models);
    Ok(())
}
