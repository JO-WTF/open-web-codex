use crate::auth::SharedAuthProvider;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::provider::Provider;
use codex_client::HttpTransport;
use codex_client::RequestTelemetry;
use codex_client::TransportError;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelsResponse;
use http::HeaderMap;
use http::Method;
use http::StatusCode;
use http::header::ETAG;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

const MAX_MODEL_CATALOG_BYTES: usize = 1024 * 1024;
const MAX_MODEL_CATALOG_ENTRIES: usize = 512;
const MAX_MODEL_ID_CHARS: usize = 256;

/// A sanitized Provider `/models` response accepted by the Runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelsCatalog {
    /// Codex's rich model metadata response.
    Rich(Vec<ModelInfo>),
    /// OpenAI-compatible responses that only advertise `data[].id`.
    OpenAiCompatible(Vec<String>),
}

/// A bounded, body-free error classification for a Provider `/models` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelsCatalogError {
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

pub struct ModelsClient<T: HttpTransport> {
    session: EndpointSession<T>,
}

impl<T: HttpTransport> ModelsClient<T> {
    pub fn new(transport: T, provider: Provider, auth: SharedAuthProvider) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
        }
    }

    pub fn with_telemetry(self, request: Option<Arc<dyn RequestTelemetry>>) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
        }
    }

    fn path() -> &'static str {
        "models"
    }

    fn append_client_version_query(req: &mut codex_client::Request, client_version: &str) {
        let separator = if req.url.contains('?') { '&' } else { '?' };
        req.url = format!("{}{}client_version={client_version}", req.url, separator);
    }

    pub fn request_url(provider: &Provider, client_version: &str) -> String {
        let mut request = provider.build_request(Method::GET, Self::path());
        Self::append_client_version_query(&mut request, client_version);
        request.url
    }

    pub async fn list_models(
        &self,
        request_url: String,
        extra_headers: HeaderMap,
    ) -> Result<(Vec<ModelInfo>, Option<String>), ApiError> {
        let resp = self
            .session
            .execute_with(
                Method::GET,
                Self::path(),
                extra_headers,
                /*body*/ None,
                move |req| {
                    req.url.clone_from(&request_url);
                },
            )
            .await?;

        let header_etag = resp
            .headers
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToString::to_string);

        let ModelsResponse { models } = serde_json::from_slice::<ModelsResponse>(&resp.body)
            .map_err(|e| {
                ApiError::Stream(format!(
                    "failed to decode models response: {e}; body: {}",
                    String::from_utf8_lossy(&resp.body)
                ))
            })?;

        Ok((models, header_etag))
    }

    /// Fetch and validate either Codex rich metadata or an OpenAI-compatible
    /// `data[].id` catalog without exposing response bytes or request details.
    pub async fn list_models_catalog(
        &self,
        request_url: String,
        extra_headers: HeaderMap,
    ) -> Result<ModelsCatalog, ModelsCatalogError> {
        let response = self
            .session
            .execute_with(
                Method::GET,
                Self::path(),
                extra_headers,
                /*body*/ None,
                move |req| {
                    req.url.clone_from(&request_url);
                },
            )
            .await
            .map_err(classify_models_transport_error)?;

        parse_models_catalog(&response.body)
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiCompatibleModelsResponse {
    data: Vec<OpenAiCompatibleModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiCompatibleModel {
    id: String,
}

fn parse_models_catalog(body: &[u8]) -> Result<ModelsCatalog, ModelsCatalogError> {
    if body.len() > MAX_MODEL_CATALOG_BYTES {
        return Err(ModelsCatalogError::IncompatibleSchema);
    }
    let value =
        serde_json::from_slice::<Value>(body).map_err(|_| ModelsCatalogError::InvalidJson)?;

    if value.get("models").is_some() {
        let response = serde_json::from_value::<ModelsResponse>(value)
            .map_err(|_| ModelsCatalogError::IncompatibleSchema)?;
        let models = validate_rich_models(response.models)?;
        return Ok(ModelsCatalog::Rich(models));
    }

    if value.get("data").is_some() {
        let response = serde_json::from_value::<OpenAiCompatibleModelsResponse>(value)
            .map_err(|_| ModelsCatalogError::IncompatibleSchema)?;
        let model_ids = validate_compatible_model_ids(response.data)?;
        return Ok(ModelsCatalog::OpenAiCompatible(model_ids));
    }

    Err(ModelsCatalogError::IncompatibleSchema)
}

fn validate_rich_models(models: Vec<ModelInfo>) -> Result<Vec<ModelInfo>, ModelsCatalogError> {
    if models.len() > MAX_MODEL_CATALOG_ENTRIES {
        return Err(ModelsCatalogError::IncompatibleSchema);
    }
    let models = models
        .into_iter()
        .filter(|model| valid_model_id(&model.slug))
        .collect::<Vec<_>>();
    if models.is_empty() {
        return Err(ModelsCatalogError::EmptyCatalog);
    }
    Ok(models)
}

fn validate_compatible_model_ids(
    models: Vec<OpenAiCompatibleModel>,
) -> Result<Vec<String>, ModelsCatalogError> {
    if models.len() > MAX_MODEL_CATALOG_ENTRIES {
        return Err(ModelsCatalogError::IncompatibleSchema);
    }
    let mut model_ids = Vec::with_capacity(models.len());
    for model in models {
        let model_id = model.id.trim();
        if valid_model_id(model_id) && !model_ids.iter().any(|id| id == model_id) {
            model_ids.push(model_id.to_string());
        }
    }
    if model_ids.is_empty() {
        return Err(ModelsCatalogError::EmptyCatalog);
    }
    Ok(model_ids)
}

fn valid_model_id(model_id: &str) -> bool {
    !model_id.trim().is_empty() && model_id.chars().count() <= MAX_MODEL_ID_CHARS
}

fn classify_models_transport_error(error: ApiError) -> ModelsCatalogError {
    match error {
        ApiError::Transport(TransportError::Http { status, .. }) => classify_http_status(status),
        ApiError::Transport(TransportError::Timeout) => ModelsCatalogError::Timeout,
        ApiError::Transport(
            TransportError::Connection(_) | TransportError::Network(_) | TransportError::Build(_),
        ) => ModelsCatalogError::Network,
        ApiError::Transport(TransportError::RetryLimit) => ModelsCatalogError::Upstream,
        ApiError::Api { status, .. } => classify_http_status(status),
        _ => ModelsCatalogError::Upstream,
    }
}

fn classify_http_status(status: StatusCode) -> ModelsCatalogError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ModelsCatalogError::Authentication,
        StatusCode::NOT_FOUND => ModelsCatalogError::NotFound,
        StatusCode::TOO_MANY_REQUESTS => ModelsCatalogError::RateLimited,
        _ => ModelsCatalogError::Upstream,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthProvider;
    use crate::provider::RetryConfig;
    use codex_client::Request;
    use codex_client::Response;
    use codex_client::StreamResponse;
    use codex_client::TransportError;
    use http::HeaderMap;
    use http::StatusCode;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    #[derive(Clone)]
    struct CapturingTransport {
        last_request: Arc<Mutex<Option<Request>>>,
        body: Arc<ModelsResponse>,
        etag: Option<String>,
    }

    impl Default for CapturingTransport {
        fn default() -> Self {
            Self {
                last_request: Arc::new(Mutex::new(None)),
                body: Arc::new(ModelsResponse { models: Vec::new() }),
                etag: None,
            }
        }
    }

    impl HttpTransport for CapturingTransport {
        async fn execute(&self, req: Request) -> Result<Response, TransportError> {
            *self.last_request.lock().unwrap() = Some(req);
            let body = serde_json::to_vec(&*self.body).unwrap();
            let mut headers = HeaderMap::new();
            if let Some(etag) = &self.etag {
                headers.insert(ETAG, etag.parse().unwrap());
            }
            Ok(Response {
                status: StatusCode::OK,
                headers,
                body: body.into(),
            })
        }

        async fn stream(&self, _req: Request) -> Result<StreamResponse, TransportError> {
            Err(TransportError::Build("stream should not run".to_string()))
        }
    }

    #[derive(Clone, Default)]
    struct DummyAuth;

    impl AuthProvider for DummyAuth {
        fn add_auth_headers(&self, _headers: &mut HeaderMap) {}
    }

    fn provider(base_url: &str) -> Provider {
        Provider {
            name: "test".to_string(),
            base_url: base_url.to_string(),
            query_params: None,
            headers: HeaderMap::new(),
            retry: RetryConfig {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                retry_429: false,
                retry_5xx: true,
                retry_transport: true,
            },
            stream_idle_timeout: Duration::from_secs(1),
        }
    }

    #[tokio::test]
    async fn appends_client_version_query() {
        let response = ModelsResponse { models: Vec::new() };

        let transport = CapturingTransport {
            last_request: Arc::new(Mutex::new(None)),
            body: Arc::new(response),
            etag: None,
        };

        let provider = provider("https://example.com/api/codex");
        let request_url = ModelsClient::<CapturingTransport>::request_url(&provider, "0.99.0");
        let client = ModelsClient::new(transport.clone(), provider, Arc::new(DummyAuth));

        let (models, _) = client
            .list_models(request_url, HeaderMap::new())
            .await
            .expect("request should succeed");

        assert_eq!(models.len(), 0);

        let url = transport
            .last_request
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .url
            .clone();
        assert_eq!(
            url,
            "https://example.com/api/codex/models?client_version=0.99.0"
        );
    }

    #[tokio::test]
    async fn parses_models_response() {
        let response = ModelsResponse {
            models: vec![
                serde_json::from_value(json!({
                    "slug": "gpt-test",
                    "display_name": "gpt-test",
                    "description": "desc",
                    "default_reasoning_level": "medium",
                    "supported_reasoning_levels": [{"effort": "low", "description": "low"}, {"effort": "medium", "description": "medium"}, {"effort": "high", "description": "high"}],
                    "shell_type": "shell_command",
                    "visibility": "list",
                    "minimal_client_version": [0, 99, 0],
                    "supported_in_api": true,
                    "priority": 1,
                    "upgrade": null,
                    "support_verbosity": false,
                    "default_verbosity": null,
                    "apply_patch_tool_type": null,
                    "truncation_policy": {"mode": "bytes", "limit": 10_000},
                    "supports_parallel_tool_calls": false,
                    "supports_image_detail_original": false,
                    "context_window": 272_000,
                    "experimental_supported_tools": [],
                }))
                .unwrap(),
            ],
        };
        let rich_body = serde_json::to_vec(&response).unwrap();

        let transport = CapturingTransport {
            last_request: Arc::new(Mutex::new(None)),
            body: Arc::new(response),
            etag: None,
        };

        let provider = provider("https://example.com/api/codex");
        let request_url = ModelsClient::<CapturingTransport>::request_url(&provider, "0.99.0");
        let client = ModelsClient::new(transport, provider, Arc::new(DummyAuth));

        let (models, _) = client
            .list_models(request_url, HeaderMap::new())
            .await
            .expect("request should succeed");

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].slug, "gpt-test");
        assert_eq!(models[0].supported_in_api, true);
        assert_eq!(models[0].priority, 1);

        let catalog = parse_models_catalog(&rich_body).expect("rich catalog should parse");
        assert!(matches!(catalog, ModelsCatalog::Rich(models) if models[0].slug == "gpt-test"));
    }

    #[tokio::test]
    async fn list_models_includes_etag() {
        let response = ModelsResponse { models: Vec::new() };

        let transport = CapturingTransport {
            last_request: Arc::new(Mutex::new(None)),
            body: Arc::new(response),
            etag: Some("\"abc\"".to_string()),
        };

        let provider = provider("https://example.com/api/codex");
        let request_url = ModelsClient::<CapturingTransport>::request_url(&provider, "0.1.0");
        let client = ModelsClient::new(transport, provider, Arc::new(DummyAuth));

        let (models, etag) = client
            .list_models(request_url, HeaderMap::new())
            .await
            .expect("request should succeed");

        assert_eq!(models.len(), 0);
        assert_eq!(etag, Some("\"abc\"".to_string()));
    }

    #[test]
    fn parses_openai_compatible_catalog_and_bounds_ids() {
        let result = parse_models_catalog(
            br#"{"data":[{"id":" first "},{"id":""},{"id":"first"},{"id":"second"}]}"#,
        )
        .expect("compatible catalog should parse");
        assert_eq!(
            result,
            ModelsCatalog::OpenAiCompatible(vec!["first".to_string(), "second".to_string()])
        );
    }

    #[test]
    fn classifies_catalog_shape_and_transport_errors_without_details() {
        assert_eq!(
            parse_models_catalog(br#"{"data":[]}"#),
            Err(ModelsCatalogError::EmptyCatalog)
        );
        assert_eq!(
            parse_models_catalog(br#"{"data":[{"name":"missing id"}]}"#),
            Err(ModelsCatalogError::IncompatibleSchema)
        );
        assert_eq!(
            parse_models_catalog(b"not-json"),
            Err(ModelsCatalogError::InvalidJson)
        );
        assert_eq!(
            classify_http_status(StatusCode::UNAUTHORIZED),
            ModelsCatalogError::Authentication
        );
        assert_eq!(
            classify_http_status(StatusCode::NOT_FOUND),
            ModelsCatalogError::NotFound
        );
        assert_eq!(
            classify_http_status(StatusCode::TOO_MANY_REQUESTS),
            ModelsCatalogError::RateLimited
        );
        assert_eq!(
            classify_http_status(StatusCode::BAD_GATEWAY),
            ModelsCatalogError::Upstream
        );
        assert_eq!(
            classify_models_transport_error(ApiError::Transport(TransportError::Timeout)),
            ModelsCatalogError::Timeout
        );
        assert_eq!(
            classify_models_transport_error(ApiError::Transport(TransportError::Network(
                "credential fragment must not escape".to_string(),
            ))),
            ModelsCatalogError::Network
        );
        assert!(!format!("{:?}", ModelsCatalogError::Authentication).contains("secret"));
    }
}
