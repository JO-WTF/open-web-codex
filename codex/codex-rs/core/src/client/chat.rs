use super::*;

const CHAT_COMPLETIONS_ENDPOINT: &str = "/chat/completions";

impl ModelClientSession {
    #[allow(clippy::too_many_arguments)]
    #[instrument(
        name = "model_client.stream_chat_completions",
        level = "info",
        skip_all,
        fields(
            model = %model_info.slug,
            wire_api = "chat",
            transport = "chat_http",
            http.method = "POST",
            api.path = "chat/completions"
        )
    )]
    pub(super) async fn stream_chat_completions(
        &self,
        prompt: &Prompt,
        model_info: &ModelInfo,
        session_telemetry: &SessionTelemetry,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
        service_tier: Option<String>,
        responses_metadata: &CodexResponsesMetadata,
        inference_trace: &InferenceTraceContext,
    ) -> Result<ResponseStream> {
        let client_setup = self.client.current_client_setup().await?;
        let transport = self
            .client
            .build_api_transport(&client_setup.api_provider, CHAT_COMPLETIONS_ENDPOINT)?;
        let request_auth_context = AuthRequestTelemetryContext::new(
            client_setup.auth.as_ref().map(CodexAuth::auth_mode),
            client_setup.api_auth.as_ref(),
            client_setup.agent_identity_telemetry.clone(),
            PendingUnauthorizedRetry::default(),
        );
        let (request_telemetry, sse_telemetry) = Self::build_streaming_telemetry(
            session_telemetry,
            request_auth_context,
            RequestRouteTelemetry::for_endpoint(CHAT_COMPLETIONS_ENDPOINT),
            self.client.state.auth_env_telemetry.clone(),
        );
        let mut request = self.client.build_responses_request(
            prompt,
            model_info,
            effort,
            summary,
            service_tier,
            responses_metadata,
        )?;
        self.client
            .prepare_response_items_for_request(&mut request.input);
        let request_session_telemetry = session_telemetry_for_request(session_telemetry, &request);
        let request = responses_request_to_chat_completions_request(request)
            .map_err(|error| self.client.state.provider.map_api_error(error))?;
        let inference_trace_attempt = inference_trace.start_attempt();
        inference_trace_attempt.record_started(&request);
        let client = ApiChatCompletionsClient::new(
            transport,
            client_setup.api_provider,
            client_setup.api_auth,
        )
        .with_telemetry(Some(request_telemetry), Some(sse_telemetry));
        let stream = client.stream_request(request).await.map_err(|error| {
            let response_debug_context = extract_response_debug_context_from_api_error(&error);
            let error = self.client.state.provider.map_api_error(error);
            inference_trace_attempt.record_failed(
                &error,
                response_debug_context.request_id.as_deref(),
                /*output_items*/ &[],
            );
            error
        })?;
        let (stream, _) = map_response_stream(
            stream,
            request_session_telemetry,
            inference_trace_attempt,
            Arc::clone(&self.client.state.provider),
        );
        Ok(stream)
    }
}
