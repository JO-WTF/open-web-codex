use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use codex_api::AgentIdentityTelemetry;
use codex_api::ModelsCatalog;
use codex_api::ModelsCatalogError;
use codex_api::ModelsClient;
use codex_api::RequestTelemetry;
use codex_api::ReqwestTransport;
use codex_api::TransportError;
use codex_api::auth_header_telemetry;
use codex_api::map_api_error;
use codex_feedback::FeedbackRequestTags;
use codex_feedback::emit_feedback_request_tags_with_auth_env;
use codex_http_client::HttpClientFactory;
use codex_login::AuthEnvTelemetry;
use codex_login::AuthManager;
use codex_login::CodexAuth;
use codex_login::collect_auth_env_telemetry;
use codex_login::default_client::create_client_for_route_without_request_logging_async;
use codex_model_provider_info::ModelProviderInfo;
use codex_models_manager::manager::ModelsEndpointClient;
use codex_models_manager::manager::ModelsEndpointFuture;
use codex_otel::TelemetryAuthMode;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CoreResult;
use codex_protocol::openai_models::ModelInfo;
use codex_response_debug_context::extract_response_debug_context;
use http::HeaderMap;
use tokio::time::timeout;

use crate::auth::agent_identity_telemetry;
use crate::auth::resolve_provider_auth;

const MODELS_REFRESH_TIMEOUT: Duration = Duration::from_secs(5);
const MODELS_ENDPOINT: &str = "/models";

/// Sanitized model metadata returned by a Provider-owned catalog request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderModelSummary {
    pub model_id: String,
    pub model_name: Option<String>,
    pub max_token_len: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub show_in_picker: bool,
    pub context_window: Option<i64>,
}

/// Body-free failure classification for a Provider-owned catalog request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderModelsError {
    Authentication,
    NotFound,
    RateLimited,
    Upstream,
    Timeout,
    Network,
    InvalidJson,
    IncompatibleSchema,
    EmptyCatalog,
}

/// Provider-owned OpenAI-compatible `/models` endpoint.
#[derive(Debug)]
pub(crate) struct OpenAiModelsEndpoint {
    provider_info: ModelProviderInfo,
    auth_manager: Option<Arc<AuthManager>>,
    transport_builder: Arc<dyn ModelsTransportBuilder>,
}

impl OpenAiModelsEndpoint {
    pub(crate) fn new(
        provider_info: ModelProviderInfo,
        auth_manager: Option<Arc<AuthManager>>,
    ) -> Self {
        Self {
            provider_info,
            auth_manager,
            transport_builder: Arc::new(RouteAwareModelsTransportBuilder),
        }
    }

    async fn auth(&self) -> Option<CodexAuth> {
        match self.auth_manager.as_ref() {
            Some(auth_manager) => auth_manager.auth().await,
            None => None,
        }
    }

    async fn uses_codex_backend(&self) -> bool {
        self.auth()
            .await
            .as_ref()
            .is_some_and(CodexAuth::uses_codex_backend)
    }

    async fn list_models(
        &self,
        client_version: &str,
        http_client_factory: HttpClientFactory,
    ) -> CoreResult<(Vec<ModelInfo>, Option<String>)> {
        let _timer =
            codex_otel::start_global_timer("codex.remote_models.fetch_update.duration_ms", &[]);
        let auth = self.auth().await;
        let auth_mode = auth.as_ref().map(CodexAuth::auth_mode);
        let api_provider = self.provider_info.to_api_provider(auth_mode)?;
        let api_auth = resolve_provider_auth(auth.as_ref(), &self.provider_info)?;
        let request_url =
            ModelsClient::<ReqwestTransport>::request_url(&api_provider, client_version);
        let auth_telemetry = auth_header_telemetry(api_auth.as_ref());
        let agent_identity_telemetry = if let Some(CodexAuth::AgentIdentity(auth)) = auth.as_ref() {
            Some(agent_identity_telemetry(auth))
        } else {
            None
        };
        let request_telemetry: Arc<dyn RequestTelemetry> = Arc::new(ModelsRequestTelemetry {
            auth_mode: auth_mode.map(|mode| TelemetryAuthMode::from(mode).to_string()),
            auth_header_attached: auth_telemetry.attached,
            auth_header_name: auth_telemetry.name,
            agent_identity_telemetry,
            auth_env: self.auth_env(),
            include_response_debug: true,
        });
        timeout(MODELS_REFRESH_TIMEOUT, async {
            let transport = self
                .transport_builder
                .build(http_client_factory, request_url.clone())
                .await?;
            let client = ModelsClient::new(transport, api_provider, api_auth)
                .with_telemetry(Some(request_telemetry));
            client
                .list_models(request_url, HeaderMap::new())
                .await
                .map_err(map_api_error)
        })
        .await
        .map_err(|_| CodexErr::Timeout)?
    }

    pub(crate) async fn list_model_catalog(
        &self,
        client_version: &str,
        http_client_factory: HttpClientFactory,
    ) -> Result<Vec<ProviderModelSummary>, ProviderModelsError> {
        let auth = self.auth().await;
        let auth_mode = auth.as_ref().map(CodexAuth::auth_mode);
        let api_provider = self
            .provider_info
            .to_api_provider(auth_mode)
            .map_err(|_| ProviderModelsError::NotFound)?;
        let api_auth = resolve_provider_auth(auth.as_ref(), &self.provider_info)
            .map_err(|_| ProviderModelsError::Authentication)?;
        let request_url =
            ModelsClient::<ReqwestTransport>::request_url(&api_provider, client_version);
        if !valid_models_request_url(&request_url) {
            return Err(ProviderModelsError::NotFound);
        }
        let auth_telemetry = auth_header_telemetry(api_auth.as_ref());
        let agent_identity_telemetry = if let Some(CodexAuth::AgentIdentity(auth)) = auth.as_ref() {
            Some(agent_identity_telemetry(auth))
        } else {
            None
        };
        let request_telemetry = Arc::new(ModelsRequestTelemetry {
            auth_mode: auth_mode.map(|mode| TelemetryAuthMode::from(mode).to_string()),
            auth_header_attached: auth_telemetry.attached,
            auth_header_name: auth_telemetry.name,
            agent_identity_telemetry,
            auth_env: self.auth_env(),
            include_response_debug: false,
        });
        let request_telemetry_for_transport: Arc<dyn RequestTelemetry> = request_telemetry.clone();
        let catalog_result = timeout(MODELS_REFRESH_TIMEOUT, async {
            let transport = self
                .transport_builder
                .build(http_client_factory, request_url.clone())
                .await
                .map_err(|_| ProviderModelsError::Network)?;
            let client = ModelsClient::new(transport, api_provider, api_auth)
                .with_telemetry(Some(request_telemetry_for_transport));
            client
                .list_models_catalog(request_url, HeaderMap::new())
                .await
                .map_err(provider_models_error_from_catalog)
        })
        .await
        .map_err(|_| ProviderModelsError::Timeout)
        .and_then(|result| result);

        request_telemetry.record_catalog_result(catalog_result.as_ref().map(|_| ()));
        let catalog = catalog_result?;

        match catalog {
            ModelsCatalog::Rich(models) => Ok(models
                .into_iter()
                .map(provider_model_summary_from_rich)
                .collect()),
            ModelsCatalog::OpenAiCompatible(model_ids) => Ok(model_ids
                .into_iter()
                .map(|model_id| ProviderModelSummary {
                    model_id,
                    model_name: None,
                    max_token_len: None,
                    max_output_tokens: None,
                    show_in_picker: true,
                    context_window: None,
                })
                .collect()),
        }
    }

    fn auth_env(&self) -> AuthEnvTelemetry {
        let codex_api_key_env_enabled = self
            .auth_manager
            .as_ref()
            .is_some_and(|auth_manager| auth_manager.codex_api_key_env_enabled());
        collect_auth_env_telemetry(&self.provider_info, codex_api_key_env_enabled)
    }
}

fn provider_models_error_from_catalog(error: ModelsCatalogError) -> ProviderModelsError {
    match error {
        ModelsCatalogError::Authentication => ProviderModelsError::Authentication,
        ModelsCatalogError::NotFound => ProviderModelsError::NotFound,
        ModelsCatalogError::RateLimited => ProviderModelsError::RateLimited,
        ModelsCatalogError::Upstream => ProviderModelsError::Upstream,
        ModelsCatalogError::Timeout => ProviderModelsError::Timeout,
        ModelsCatalogError::Network => ProviderModelsError::Network,
        ModelsCatalogError::InvalidJson => ProviderModelsError::InvalidJson,
        ModelsCatalogError::IncompatibleSchema => ProviderModelsError::IncompatibleSchema,
        ModelsCatalogError::EmptyCatalog => ProviderModelsError::EmptyCatalog,
    }
}

fn valid_models_request_url(request_url: &str) -> bool {
    let Ok(uri) = request_url.parse::<http::Uri>() else {
        return false;
    };
    matches!(uri.scheme_str(), Some("http") | Some("https")) && uri.authority().is_some()
}

fn provider_model_summary_from_rich(
    model: codex_protocol::openai_models::ModelInfo,
) -> ProviderModelSummary {
    let context_window = model.resolved_context_window().filter(|window| *window > 0);
    let max_token_len = matches!(
        model.truncation_policy.mode,
        codex_protocol::openai_models::TruncationMode::Tokens
    )
    .then_some(model.truncation_policy.limit)
    .filter(|limit| *limit > 0);
    ProviderModelSummary {
        model_id: model.slug,
        model_name: Some(model.display_name),
        max_token_len,
        max_output_tokens: None,
        show_in_picker: matches!(
            model.visibility,
            codex_protocol::openai_models::ModelVisibility::List
        ) && model.supported_in_api,
        context_window,
    }
}

impl ModelsEndpointClient for OpenAiModelsEndpoint {
    fn has_command_auth(&self) -> bool {
        self.provider_info.has_command_auth()
    }

    fn uses_codex_backend(&self) -> ModelsEndpointFuture<'_, bool> {
        Box::pin(OpenAiModelsEndpoint::uses_codex_backend(self))
    }

    fn list_models<'a>(
        &'a self,
        client_version: &'a str,
        http_client_factory: HttpClientFactory,
    ) -> ModelsEndpointFuture<'a, CoreResult<(Vec<ModelInfo>, Option<String>)>> {
        Box::pin(OpenAiModelsEndpoint::list_models(
            self,
            client_version,
            http_client_factory,
        ))
    }
}

type ModelsTransportFuture<'a> =
    Pin<Box<dyn Future<Output = std::io::Result<ReqwestTransport>> + Send + 'a>>;

/// Builds the concrete transport selected for one models request.
///
/// Implementations must honor the supplied request-time client factory and exact request URL.
trait ModelsTransportBuilder: fmt::Debug + Send + Sync {
    fn build(
        &self,
        http_client_factory: HttpClientFactory,
        request_url: String,
    ) -> ModelsTransportFuture<'_>;
}

#[derive(Debug)]
struct RouteAwareModelsTransportBuilder;

impl ModelsTransportBuilder for RouteAwareModelsTransportBuilder {
    fn build(
        &self,
        http_client_factory: HttpClientFactory,
        request_url: String,
    ) -> ModelsTransportFuture<'_> {
        Box::pin(async move {
            let client = create_client_for_route_without_request_logging_async(
                http_client_factory,
                request_url,
                codex_http_client::ClientRouteClass::Api,
            )
            .await?;
            Ok(ReqwestTransport::from_http_client(client))
        })
    }
}

#[derive(Clone)]
struct ModelsRequestTelemetry {
    auth_mode: Option<String>,
    auth_header_attached: bool,
    auth_header_name: Option<&'static str>,
    agent_identity_telemetry: Option<AgentIdentityTelemetry>,
    auth_env: AuthEnvTelemetry,
    include_response_debug: bool,
}

impl ModelsRequestTelemetry {
    fn response_debug_context(
        &self,
        error: Option<&TransportError>,
    ) -> codex_response_debug_context::ResponseDebugContext {
        if self.include_response_debug {
            error
                .map(extract_response_debug_context)
                .unwrap_or_default()
        } else {
            Default::default()
        }
    }

    fn record_catalog_result(&self, result: Result<(), &ProviderModelsError>) {
        let (success, error_category) = match result {
            Ok(()) => (true, None),
            Err(error) => (false, Some(provider_models_error_category(error))),
        };
        tracing::event!(
            target: "codex_otel.trace_safe",
            tracing::Level::INFO,
            event.name = "codex.model_catalog",
            endpoint = MODELS_ENDPOINT,
            success = success,
            error.category = error_category,
        );
    }
}

fn provider_models_error_category(error: &ProviderModelsError) -> &'static str {
    match error {
        ProviderModelsError::Authentication => "authentication",
        ProviderModelsError::NotFound => "not_found",
        ProviderModelsError::RateLimited => "rate_limited",
        ProviderModelsError::Upstream => "upstream",
        ProviderModelsError::Timeout => "timeout",
        ProviderModelsError::Network => "network",
        ProviderModelsError::InvalidJson => "invalid_json",
        ProviderModelsError::IncompatibleSchema => "incompatible_schema",
        ProviderModelsError::EmptyCatalog => "empty_catalog",
    }
}

fn models_transport_error_category(error: &TransportError) -> &'static str {
    match error {
        TransportError::Http { .. } => "http",
        TransportError::RetryLimit => "retry_limit",
        TransportError::Timeout => "timeout",
        TransportError::Connection(_) => "connection",
        TransportError::Network(_) => "network",
        TransportError::Build(_) => "build",
    }
}

impl RequestTelemetry for ModelsRequestTelemetry {
    fn on_request(
        &self,
        attempt: u64,
        status: Option<http::StatusCode>,
        error: Option<&TransportError>,
        duration: Duration,
    ) {
        let success = status.is_some_and(|code| code.is_success()) && error.is_none();
        let error_category = error.map(models_transport_error_category);
        let response_debug = self.response_debug_context(error);
        let status = status.map(|status| status.as_u16());
        tracing::event!(
            target: "codex_otel.log_only",
            tracing::Level::INFO,
            event.name = "codex.api_request",
            duration_ms = %duration.as_millis(),
            http.response.status_code = status,
            success = success,
            error.category = error_category,
            attempt = attempt,
            endpoint = MODELS_ENDPOINT,
            auth.header_attached = self.auth_header_attached,
            auth.header_name = self.auth_header_name,
            auth.env_openai_api_key_present = self.auth_env.openai_api_key_env_present,
            auth.env_codex_api_key_present = self.auth_env.codex_api_key_env_present,
            auth.env_codex_api_key_enabled = self.auth_env.codex_api_key_env_enabled,
            auth.env_provider_key_name = self.auth_env.provider_env_key_name.as_deref(),
            auth.env_provider_key_present = self.auth_env.provider_env_key_present,
            auth.env_refresh_token_url_override_present = self.auth_env.refresh_token_url_override_present,
            auth.request_id = response_debug.request_id.as_deref(),
            auth.cf_ray = response_debug.cf_ray.as_deref(),
            auth.error = response_debug.auth_error.as_deref(),
            auth.error_code = response_debug.auth_error_code.as_deref(),
            auth.mode = self.auth_mode.as_deref(),
            auth.agent_id = self.agent_identity_telemetry.as_ref().map(|metadata| metadata.agent_id.as_str()),
            auth.task_id = self.agent_identity_telemetry.as_ref().map(|metadata| metadata.task_id.as_str()),
        );
        tracing::event!(
            target: "codex_otel.trace_safe",
            tracing::Level::INFO,
            event.name = "codex.api_request",
            duration_ms = %duration.as_millis(),
            http.response.status_code = status,
            success = success,
            error.category = error_category,
            attempt = attempt,
            endpoint = MODELS_ENDPOINT,
            auth.header_attached = self.auth_header_attached,
            auth.header_name = self.auth_header_name,
            auth.env_openai_api_key_present = self.auth_env.openai_api_key_env_present,
            auth.env_codex_api_key_present = self.auth_env.codex_api_key_env_present,
            auth.env_codex_api_key_enabled = self.auth_env.codex_api_key_env_enabled,
            auth.env_provider_key_name = self.auth_env.provider_env_key_name.as_deref(),
            auth.env_provider_key_present = self.auth_env.provider_env_key_present,
            auth.env_refresh_token_url_override_present = self.auth_env.refresh_token_url_override_present,
            auth.request_id = response_debug.request_id.as_deref(),
            auth.cf_ray = response_debug.cf_ray.as_deref(),
            auth.error = response_debug.auth_error.as_deref(),
            auth.error_code = response_debug.auth_error_code.as_deref(),
            auth.mode = self.auth_mode.as_deref(),
            auth.agent_id = self.agent_identity_telemetry.as_ref().map(|metadata| metadata.agent_id.as_str()),
            auth.task_id = self.agent_identity_telemetry.as_ref().map(|metadata| metadata.task_id.as_str()),
        );
        emit_feedback_request_tags_with_auth_env(
            &FeedbackRequestTags {
                endpoint: MODELS_ENDPOINT,
                auth_header_attached: self.auth_header_attached,
                auth_header_name: self.auth_header_name,
                auth_mode: self.auth_mode.as_deref(),
                auth_retry_after_unauthorized: None,
                auth_recovery_mode: None,
                auth_recovery_phase: None,
                auth_connection_reused: None,
                auth_request_id: response_debug.request_id.as_deref(),
                auth_cf_ray: response_debug.cf_ray.as_deref(),
                auth_error: response_debug.auth_error.as_deref(),
                auth_error_code: response_debug.auth_error_code.as_deref(),
                auth_recovery_followup_success: None,
                auth_recovery_followup_status: None,
            },
            &self.auth_env,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::io::Write;
    use std::num::NonZeroU64;
    use std::sync::Mutex;

    use super::*;
    use codex_http_client::OutboundProxyPolicy;
    use codex_login::default_client::create_client;
    use codex_protocol::config_types::ModelProviderAuthInfo;
    use codex_protocol::openai_models::ModelsResponse;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use tracing_subscriber::layer::SubscriberExt;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::header;
    use wiremock::matchers::method;
    use wiremock::matchers::path;
    use wiremock::matchers::query_param;

    #[derive(Clone)]
    struct TestLogWriter {
        buffer: Arc<Mutex<Vec<u8>>>,
    }

    struct TestLogSink {
        buffer: Arc<Mutex<Vec<u8>>>,
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for TestLogWriter {
        type Writer = TestLogSink;

        fn make_writer(&'a self) -> Self::Writer {
            TestLogSink {
                buffer: Arc::clone(&self.buffer),
            }
        }
    }

    impl Write for TestLogSink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.buffer
                .lock()
                .expect("telemetry log buffer lock")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct RecordingTransportBuilder {
        observed_request: Arc<Mutex<Option<(OutboundProxyPolicy, String)>>>,
    }

    impl ModelsTransportBuilder for RecordingTransportBuilder {
        fn build(
            &self,
            http_client_factory: HttpClientFactory,
            request_url: String,
        ) -> ModelsTransportFuture<'_> {
            let observed_request = Arc::clone(&self.observed_request);
            Box::pin(async move {
                *observed_request
                    .lock()
                    .expect("observed request lock should not be poisoned") =
                    Some((http_client_factory.outbound_proxy_policy(), request_url));
                Ok(ReqwestTransport::from_http_client(create_client()))
            })
        }
    }

    fn provider_info_with_command_auth() -> ModelProviderInfo {
        ModelProviderInfo {
            auth: Some(ModelProviderAuthInfo {
                command: "print-token".to_string(),
                args: Vec::new(),
                timeout_ms: NonZeroU64::new(5_000).expect("timeout should be non-zero"),
                refresh_interval_ms: 300_000,
                cwd: std::env::current_dir()
                    .expect("current dir should be available")
                    .try_into()
                    .expect("current dir should be absolute"),
            }),
            requires_openai_auth: false,
            ..ModelProviderInfo::create_openai_provider(/*base_url*/ None)
        }
    }

    #[test]
    fn command_auth_provider_reports_command_auth_without_cached_auth() {
        let endpoint = OpenAiModelsEndpoint::new(
            provider_info_with_command_auth(),
            /*auth_manager*/ None,
        );

        assert!(endpoint.has_command_auth());
    }

    #[test]
    fn provider_without_command_auth_reports_no_command_auth() {
        let endpoint = OpenAiModelsEndpoint::new(
            ModelProviderInfo::create_openai_provider(/*base_url*/ None),
            /*auth_manager*/ None,
        );

        assert!(!endpoint.has_command_auth());
    }

    fn rich_model() -> ModelInfo {
        serde_json::from_value(json!({
            "slug": "gpt-test",
            "display_name": "gpt-test",
            "description": "desc",
            "default_reasoning_level": "medium",
            "supported_reasoning_levels": [{"effort": "low", "description": "low"}],
            "shell_type": "shell_command",
            "visibility": "list",
            "supported_in_api": true,
            "priority": 1,
            "support_verbosity": false,
            "default_verbosity": null,
            "apply_patch_tool_type": null,
            "truncation_policy": {"mode": "tokens", "limit": 10_000},
            "supports_parallel_tool_calls": false,
            "supports_image_detail_original": false,
            "context_window": 272_000,
            "experimental_supported_tools": [],
        }))
        .expect("rich model fixture should deserialize")
    }

    #[test]
    fn rich_summary_copies_display_name_without_normalizing() {
        let mut model = rich_model();
        model.display_name = "  Display Name  ".to_string();
        assert_eq!(
            provider_model_summary_from_rich(model).model_name,
            Some("  Display Name  ".to_string())
        );
    }

    #[tokio::test]
    async fn model_request_uses_request_time_proxy_policy_and_exact_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .and(query_param("client_version", "0.0.0"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(ModelsResponse { models: Vec::new() }),
            )
            .expect(1)
            .mount(&server)
            .await;

        let observed_request = Arc::new(Mutex::new(None));
        let endpoint = OpenAiModelsEndpoint {
            provider_info: ModelProviderInfo::create_openai_provider(Some(server.uri())),
            auth_manager: None,
            transport_builder: Arc::new(RecordingTransportBuilder {
                observed_request: Arc::clone(&observed_request),
            }),
        };

        endpoint
            .list_models(
                "0.0.0",
                HttpClientFactory::new(OutboundProxyPolicy::RespectSystemProxy),
            )
            .await
            .expect("models request should succeed");

        assert_eq!(
            *observed_request
                .lock()
                .expect("observed request lock should not be poisoned"),
            Some((
                OutboundProxyPolicy::RespectSystemProxy,
                format!("{}/models?client_version=0.0.0", server.uri()),
            ))
        );
    }

    #[tokio::test]
    async fn fresh_catalog_accepts_compatible_shape_and_attaches_provider_auth() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .and(header("authorization", "Bearer provider-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": "compatible-model"}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let mut provider_info = ModelProviderInfo::create_openai_provider(Some(server.uri()));
        provider_info.experimental_bearer_token = Some("provider-secret".to_string());
        provider_info.request_max_retries = Some(0);
        let endpoint = OpenAiModelsEndpoint::new(provider_info, None);

        let models = endpoint
            .list_model_catalog(
                "0.0.0",
                HttpClientFactory::new(OutboundProxyPolicy::RespectSystemProxy),
            )
            .await
            .expect("compatible catalog should succeed");

        assert_eq!(
            models,
            vec![ProviderModelSummary {
                model_id: "compatible-model".to_string(),
                model_name: None,
                max_token_len: None,
                max_output_tokens: None,
                show_in_picker: true,
                context_window: None,
            }]
        );
    }

    #[tokio::test]
    async fn fresh_catalog_maps_invalid_base_url_to_not_found() {
        let provider_info =
            ModelProviderInfo::create_openai_provider(Some("not a valid absolute URL".to_string()));
        let endpoint = OpenAiModelsEndpoint::new(provider_info, None);

        let error = endpoint
            .list_model_catalog(
                "0.0.0",
                HttpClientFactory::new(OutboundProxyPolicy::RespectSystemProxy),
            )
            .await
            .expect_err("invalid base URL should be rejected before transport");

        assert_eq!(error, ProviderModelsError::NotFound);
    }

    #[tokio::test]
    async fn fresh_catalog_maps_http_failures_without_response_details() {
        let cases = [
            (401, ProviderModelsError::Authentication),
            (403, ProviderModelsError::Authentication),
            (404, ProviderModelsError::NotFound),
            (429, ProviderModelsError::RateLimited),
            (408, ProviderModelsError::Timeout),
            (504, ProviderModelsError::Timeout),
            (500, ProviderModelsError::Upstream),
            (400, ProviderModelsError::Upstream),
        ];
        for (status, expected) in cases {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/models"))
                .respond_with(
                    ResponseTemplate::new(status)
                        .set_body_string("authorization=provider-secret url=https://secret.test"),
                )
                .mount(&server)
                .await;
            let mut provider_info = ModelProviderInfo::create_openai_provider(Some(server.uri()));
            provider_info.experimental_bearer_token = Some("provider-secret".to_string());
            provider_info.request_max_retries = Some(0);
            let endpoint = OpenAiModelsEndpoint::new(provider_info, None);

            let error = endpoint
                .list_model_catalog(
                    "0.0.0",
                    HttpClientFactory::new(OutboundProxyPolicy::RespectSystemProxy),
                )
                .await
                .expect_err("HTTP error should be classified");

            assert_eq!(error, expected);
            assert!(!format!("{error:?}").contains("provider-secret"));
            assert!(!format!("{error:?}").contains("secret.test"));
        }
    }

    #[tokio::test]
    async fn fresh_catalog_rejects_an_oversized_http_body_before_buffering_it() {
        let server = MockServer::start().await;
        let oversized_body = vec![b'x'; 1024 * 1024 + 1];
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized_body))
            .expect(1)
            .mount(&server)
            .await;

        let mut provider_info = ModelProviderInfo::create_openai_provider(Some(server.uri()));
        provider_info.request_max_retries = Some(0);
        let endpoint = OpenAiModelsEndpoint::new(provider_info, None);

        let error = endpoint
            .list_model_catalog(
                "0.0.0",
                HttpClientFactory::new(OutboundProxyPolicy::RespectSystemProxy),
            )
            .await
            .expect_err("oversized catalog should be rejected");

        assert_eq!(error, ProviderModelsError::IncompatibleSchema);
        assert!(!format!("{error:?}").contains('x'));
    }

    #[test]
    fn fresh_catalog_telemetry_exposes_only_typed_categories_and_no_response_debug() {
        let telemetry = ModelsRequestTelemetry {
            auth_mode: None,
            auth_header_attached: false,
            auth_header_name: None,
            agent_identity_telemetry: None,
            auth_env: collect_auth_env_telemetry(
                &ModelProviderInfo::create_openai_provider(None),
                false,
            ),
            include_response_debug: false,
        };
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "response-header-canary".parse().unwrap());
        let transport = TransportError::Http {
            status: http::StatusCode::BAD_GATEWAY,
            url: Some("https://user:credential@example.test/models?canary".to_string()),
            headers: Some(headers),
            body: Some("response-body-canary".to_string()),
        };

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(TestLogWriter {
                    buffer: Arc::clone(&buffer),
                }),
        );
        let _guard = tracing::subscriber::set_default(subscriber);

        telemetry.on_request(1, None, Some(&transport), Duration::from_millis(1));
        telemetry.record_catalog_result(Err(&ProviderModelsError::IncompatibleSchema));

        assert_eq!(models_transport_error_category(&transport), "http");
        assert_eq!(
            models_transport_error_category(&TransportError::Network(
                "network-url-canary".to_string()
            )),
            "network"
        );
        assert_eq!(
            models_transport_error_category(&TransportError::Build(
                "build-body-canary".to_string()
            )),
            "build"
        );
        assert_eq!(
            telemetry.response_debug_context(Some(&transport)),
            Default::default()
        );
        assert_eq!(
            provider_models_error_category(&ProviderModelsError::IncompatibleSchema),
            "incompatible_schema"
        );

        let logs = String::from_utf8(buffer.lock().expect("telemetry log buffer lock").clone())
            .expect("telemetry logs should be UTF-8");
        for marker in [
            "credential@example.test",
            "canary",
            "response-header-canary",
            "response-body-canary",
            "network-url-canary",
            "build-body-canary",
        ] {
            assert!(!logs.contains(marker), "telemetry leaked marker: {marker}");
        }
    }
}
