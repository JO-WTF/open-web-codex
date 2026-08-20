use anyhow::Result;
use app_test_support::TestAppServer;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ModelProviderModelsListFailure;
use codex_app_server_protocol::ModelProviderModelsListParams;
use codex_app_server_protocol::ModelProviderModelsListResponse;
use codex_app_server_protocol::ModelProviderModelsListResult;
use tempfile::TempDir;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;

fn provider_config(provider_id: &str, server_uri: &str, token: &str) -> String {
    format!(
        r#"
model_provider = "{provider_id}"

[model_providers.{provider_id}]
name = "{provider_id}"
base_url = "{server_uri}"
experimental_bearer_token = "{token}"
wire_api = "chat"
request_max_retries = 0
"#
    )
}

async fn app_with_provider(
    provider_id: &str,
    server_uri: &str,
    token: &str,
) -> Result<(TestAppServer, TempDir)> {
    let codex_home = TempDir::new()?;
    std::fs::write(
        codex_home.path().join("config.toml"),
        provider_config(provider_id, server_uri, token),
    )?;
    let app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build_initialized()
        .await?;
    Ok((app, codex_home))
}

#[tokio::test]
async fn fresh_catalog_targets_requested_provider_without_switching_current() -> Result<()> {
    let current_server = MockServer::start().await;
    let target_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .and(header("authorization", "Bearer target-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{"id": "target-model"}]
        })))
        .expect(1)
        .mount(&target_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&current_server)
        .await;

    let codex_home = TempDir::new()?;
    std::fs::write(
        codex_home.path().join("config.toml"),
        format!(
            r#"
model_provider = "current"

[model_providers.current]
name = "current"
base_url = "{}"
experimental_bearer_token = "current-secret"
wire_api = "chat"
request_max_retries = 0

[model_providers.target]
name = "target"
base_url = "{}"
experimental_bearer_token = "target-secret"
wire_api = "chat"
request_max_retries = 0
"#,
            current_server.uri(),
            target_server.uri(),
        ),
    )?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build_initialized()
        .await?;

    let response: ModelProviderModelsListResponse = app
        .request(|request_id| ClientRequest::ModelProviderModelsList {
            request_id,
            params: ModelProviderModelsListParams {
                provider_id: "target".to_string(),
            },
        })
        .await?;

    assert_eq!(
        response.result,
        ModelProviderModelsListResult::Success {
            models: vec![codex_app_server_protocol::ModelProviderModelSummary {
                model_id: "target-model".to_string(),
                model_name: None,
                max_token_len: None,
                max_output_tokens: None,
                show_in_picker: true,
                context_window: None,
                supports_search_tool: false,
            }]
        }
    );

    let second: ModelProviderModelsListResponse = app
        .request(|request_id| ClientRequest::ModelProviderModelsList {
            request_id,
            params: ModelProviderModelsListParams {
                provider_id: "missing".to_string(),
            },
        })
        .await?;
    assert_eq!(
        second.result,
        ModelProviderModelsListResult::Failure {
            error: ModelProviderModelsListFailure::NotFound,
        }
    );

    for provider_id in ["", "  ", " target ", "target\n"] {
        let response: ModelProviderModelsListResponse = app
            .request(|request_id| ClientRequest::ModelProviderModelsList {
                request_id,
                params: ModelProviderModelsListParams {
                    provider_id: provider_id.to_string(),
                },
            })
            .await?;
        assert_eq!(
            response.result,
            ModelProviderModelsListResult::Failure {
                error: ModelProviderModelsListFailure::NotFound,
            }
        );
    }
    let response: ModelProviderModelsListResponse = app
        .request(|request_id| ClientRequest::ModelProviderModelsList {
            request_id,
            params: ModelProviderModelsListParams {
                provider_id: "x".repeat(129),
            },
        })
        .await?;
    assert_eq!(
        response.result,
        ModelProviderModelsListResult::Failure {
            error: ModelProviderModelsListFailure::NotFound,
        }
    );
    Ok(())
}

#[tokio::test]
async fn fresh_catalog_returns_typed_empty_and_invalid_json_failures() -> Result<()> {
    for (body, expected) in [
        (
            serde_json::json!({"data": []}),
            ModelProviderModelsListFailure::EmptyCatalog,
        ),
        (
            serde_json::json!({"data": [{"name": "missing id"}]}),
            ModelProviderModelsListFailure::IncompatibleSchema,
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let (mut app, _codex_home) = app_with_provider("target", &server.uri(), "secret").await?;

        let response: ModelProviderModelsListResponse = app
            .request(|request_id| ClientRequest::ModelProviderModelsList {
                request_id,
                params: ModelProviderModelsListParams {
                    provider_id: "target".to_string(),
                },
            })
            .await?;
        assert_eq!(
            response.result,
            ModelProviderModelsListResult::Failure { error: expected }
        );
    }

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
        .mount(&server)
        .await;
    let (mut app, _codex_home) = app_with_provider("target", &server.uri(), "secret").await?;
    let response: ModelProviderModelsListResponse = app
        .request(|request_id| ClientRequest::ModelProviderModelsList {
            request_id,
            params: ModelProviderModelsListParams {
                provider_id: "target".to_string(),
            },
        })
        .await?;
    assert_eq!(
        response.result,
        ModelProviderModelsListResult::Failure {
            error: ModelProviderModelsListFailure::InvalidJson,
        }
    );
    Ok(())
}
