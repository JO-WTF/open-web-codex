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
model = "deepseek-v4-flash"

[model_providers.deepseek-e2e]
name = "DeepSeek E2E"
base_url = "https://api.deepseek.com"
experimental_bearer_token = "must-not-be-returned"
query_params = { api_version = "must-not-return-query" }
http_headers = { Authorization = "must-not-return-header" }
env_http_headers = { X-Provider-Key = "MUST_NOT_RETURN_ENV_HEADER" }
wire_api = "chat"
supports_function_tools = true
models = [
  { model_id = "deepseek-v4-flash", model_name = "DeepSeek V4 Flash", max_token_len = 128000, max_output_tokens = 8192, show_in_picker = true, context_window = 128000, supports_search_tool = true },
  { model_id = "deepseek-hidden", show_in_picker = false, supports_search_tool = false },
]

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
    assert_eq!(
        response.current_model_id.as_deref(),
        Some("deepseek-v4-flash")
    );
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
    assert_eq!(deepseek.model_count, 2);
    assert_eq!(
        deepseek
            .models
            .iter()
            .map(|model| model.model_id.as_str())
            .collect::<Vec<_>>(),
        vec!["deepseek-v4-flash", "deepseek-hidden"],
        "configured model order must be preserved without a fresh catalog fetch",
    );
    assert_eq!(
        deepseek.models[0].model_name.as_deref(),
        Some("DeepSeek V4 Flash")
    );
    assert_eq!(deepseek.models[0].max_token_len, Some(128_000));
    assert_eq!(deepseek.models[0].max_output_tokens, Some(8_192));
    assert!(deepseek.models[0].show_in_picker);
    assert_eq!(deepseek.models[0].context_window, Some(128_000));
    assert!(deepseek.models[0].supports_search_tool);
    assert!(!deepseek.models[1].show_in_picker);
    assert!(!deepseek.models[1].supports_search_tool);
    let serialized = serde_json::to_string(&response)?;
    for secret in [
        "must-not-be-returned",
        "must-not-return-query",
        "must-not-return-header",
        "MUST_NOT_RETURN_ENV_HEADER",
    ] {
        assert!(!serialized.contains(secret));
    }

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
