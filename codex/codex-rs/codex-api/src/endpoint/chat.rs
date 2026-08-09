use crate::auth::SharedAuthProvider;
use crate::chat_translate::ChatCompletionsApiRequest;
use crate::chat_translate::ChatToolTarget;
use crate::common::ResponseStream;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::provider::Provider;
use crate::sse::spawn_chat_response_stream;
use crate::telemetry::SseTelemetry;
use codex_client::EncodedJsonBody;
use codex_client::HttpTransport;
use codex_client::RequestTelemetry;
use http::HeaderValue;
use http::Method;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::instrument;

pub struct ChatCompletionsClient<T: HttpTransport> {
    session: EndpointSession<T>,
    sse_telemetry: Option<Arc<dyn SseTelemetry>>,
}

impl<T: HttpTransport> ChatCompletionsClient<T> {
    pub fn new(transport: T, provider: Provider, auth: SharedAuthProvider) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
            sse_telemetry: None,
        }
    }

    pub fn with_telemetry(
        self,
        request: Option<Arc<dyn RequestTelemetry>>,
        sse: Option<Arc<dyn SseTelemetry>>,
    ) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
            sse_telemetry: sse,
        }
    }

    #[instrument(
        name = "chat_completions.stream_request",
        level = "info",
        skip_all,
        fields(transport = "chat_http", http.method = "POST", api.path = "chat/completions")
    )]
    pub async fn stream_request(
        &self,
        request: ChatCompletionsApiRequest,
    ) -> Result<ResponseStream, ApiError> {
        let tool_targets = request
            .tools
            .iter()
            .map(|tool| (tool.function.name.clone(), tool.target.clone()))
            .collect::<HashMap<String, ChatToolTarget>>();
        let body = EncodedJsonBody::encode(&request).map_err(|error| {
            ApiError::Stream(format!(
                "failed to encode chat completions request: {error}"
            ))
        })?;

        let response = self
            .session
            .stream_encoded_json_with(
                Method::POST,
                "chat/completions",
                Default::default(),
                Some(body),
                |request| {
                    request.headers.insert(
                        http::header::ACCEPT,
                        HeaderValue::from_static("text/event-stream"),
                    );
                },
            )
            .await?;

        Ok(spawn_chat_response_stream(
            response,
            self.session.provider().stream_idle_timeout,
            self.sse_telemetry.clone(),
            tool_targets,
        ))
    }
}
