use crate::auth::SharedAuthProvider;
use crate::error::ApiError;
use crate::provider::Provider;
use crate::telemetry::run_with_request_telemetry;
use bytes::Bytes;
use codex_client::EncodedJsonBody;
use codex_client::HttpTransport;
use codex_client::Request;
use codex_client::RequestBody;
use codex_client::RequestTelemetry;
use codex_client::Response;
use codex_client::StreamResponse;
use codex_client::TransportError;
use futures::StreamExt;
use http::HeaderMap;
use http::Method;
use serde_json::Value;
use std::sync::Arc;
use tracing::instrument;

/// Typed failures for a unary response that is consumed through a bounded
/// streaming body reader.
///
/// This is intentionally local to the endpoint session.  Existing unary
/// transport callers continue to use [`HttpTransport::execute`] with its
/// established response semantics; only callers that opt into this helper
/// receive the explicit body-size terminal result.
#[derive(Debug)]
pub(crate) enum BoundedResponseError {
    Api(ApiError),
    BodyTooLarge,
}

impl From<ApiError> for BoundedResponseError {
    fn from(error: ApiError) -> Self {
        Self::Api(error)
    }
}

pub(crate) struct EndpointSession<T: HttpTransport> {
    transport: T,
    provider: Provider,
    auth: SharedAuthProvider,
    request_telemetry: Option<Arc<dyn RequestTelemetry>>,
}

impl<T: HttpTransport> EndpointSession<T> {
    pub(crate) fn new(transport: T, provider: Provider, auth: SharedAuthProvider) -> Self {
        Self {
            transport,
            provider,
            auth,
            request_telemetry: None,
        }
    }

    pub(crate) fn with_request_telemetry(
        mut self,
        request: Option<Arc<dyn RequestTelemetry>>,
    ) -> Self {
        self.request_telemetry = request;
        self
    }

    pub(crate) fn provider(&self) -> &Provider {
        &self.provider
    }

    fn make_request(
        &self,
        method: &Method,
        path: &str,
        extra_headers: &HeaderMap,
        body: Option<&RequestBody>,
    ) -> Request {
        let mut req = self.provider.build_request(method.clone(), path);
        req.headers.extend(extra_headers.clone());
        if let Some(body) = body {
            req.body = Some(body.clone());
        }
        req
    }

    pub(crate) async fn execute(
        &self,
        method: Method,
        path: &str,
        extra_headers: HeaderMap,
        body: Option<Value>,
    ) -> Result<Response, ApiError> {
        self.execute_with(method, path, extra_headers, body, |_| {})
            .await
    }

    #[instrument(
        name = "endpoint_session.execute_with",
        level = "info",
        skip_all,
        fields(http.method = %method, api.path = path)
    )]
    pub(crate) async fn execute_with<C>(
        &self,
        method: Method,
        path: &str,
        extra_headers: HeaderMap,
        body: Option<Value>,
        configure: C,
    ) -> Result<Response, ApiError>
    where
        C: Fn(&mut Request),
    {
        let body = body.map(RequestBody::Json);
        let make_request = || {
            let mut req = self.make_request(&method, path, &extra_headers, body.as_ref());
            configure(&mut req);
            req
        };

        let response = run_with_request_telemetry(
            self.provider.retry.to_policy(),
            self.request_telemetry.clone(),
            make_request,
            |req| {
                let auth = self.auth.clone();
                let transport = &self.transport;
                async move {
                    let req = auth.apply_auth(req).await.map_err(TransportError::from)?;
                    transport.execute(req).await
                }
            },
        )
        .await?;

        Ok(response)
    }

    /// Executes a unary request by consuming its response body as a bounded
    /// stream.  The stream is dropped immediately after the first chunk that
    /// would exceed `max_body_bytes`; no later chunks are polled or buffered.
    ///
    /// This helper is for endpoints with a strict response-size contract. It
    /// deliberately does not change [`Self::execute_with`] or the shared
    /// transport's ordinary unary behavior.
    #[instrument(
        name = "endpoint_session.execute_bounded_with",
        level = "info",
        skip_all,
        fields(http.method = %method, api.path = path)
    )]
    pub(crate) async fn execute_bounded_with<C>(
        &self,
        method: Method,
        path: &str,
        extra_headers: HeaderMap,
        body: Option<Value>,
        max_body_bytes: usize,
        configure: C,
    ) -> Result<Response, BoundedResponseError>
    where
        C: Fn(&mut Request),
    {
        let body = body.map(RequestBody::Json);
        let make_request = || {
            let mut req = self.make_request(&method, path, &extra_headers, body.as_ref());
            configure(&mut req);
            req
        };

        let stream = run_with_request_telemetry(
            self.provider.retry.to_policy(),
            self.request_telemetry.clone(),
            make_request,
            |req| {
                let auth = self.auth.clone();
                let transport = &self.transport;
                async move {
                    let req = auth.apply_auth(req).await.map_err(TransportError::from)?;
                    transport.stream(req).await
                }
            },
        )
        .await
        .map_err(ApiError::from)?;

        let status = stream.status;
        let headers = stream.headers;
        let mut bytes = stream.bytes;
        let Some(max_plus_one) = max_body_bytes.checked_add(1) else {
            return Err(BoundedResponseError::BodyTooLarge);
        };
        let mut body = Vec::with_capacity(max_plus_one.min(8 * 1024));

        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.map_err(ApiError::Transport)?;
            let next_len = body.len().saturating_add(chunk.len());
            if next_len >= max_plus_one {
                // Returning drops the stream here, so no subsequent body
                // chunk is read after the first byte above the configured
                // maximum.
                return Err(BoundedResponseError::BodyTooLarge);
            }
            body.extend_from_slice(&chunk);
        }

        Ok(Response {
            status,
            headers,
            body: Bytes::from(body),
        })
    }

    #[instrument(
        name = "endpoint_session.stream_encoded_json_with",
        level = "info",
        skip_all,
        fields(http.method = %method, api.path = path)
    )]
    pub(crate) async fn stream_encoded_json_with<C>(
        &self,
        method: Method,
        path: &str,
        extra_headers: HeaderMap,
        body: Option<EncodedJsonBody>,
        configure: C,
    ) -> Result<StreamResponse, ApiError>
    where
        C: Fn(&mut Request),
    {
        let body = body.map(RequestBody::EncodedJson);
        let mut request = self.make_request(&method, path, &extra_headers, body.as_ref());
        configure(&mut request);
        let request = request.into_prepared().map_err(TransportError::Build)?;
        let make_request = || request.clone();

        let stream = run_with_request_telemetry(
            self.provider.retry.to_policy(),
            self.request_telemetry.clone(),
            make_request,
            |req| {
                let auth = self.auth.clone();
                let transport = &self.transport;
                async move {
                    let req = auth.apply_auth(req).await.map_err(TransportError::from)?;
                    transport.stream(req).await
                }
            },
        )
        .await?;

        Ok(stream)
    }
}
