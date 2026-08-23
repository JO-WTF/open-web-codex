use codex_api::ApiError;
use codex_api::TransportError;
use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrorDetails;
use http::StatusCode;

pub(super) const BEDROCK_EXPIRED_SIGNATURE_MESSAGE: &str = concat!(
    "Amazon Bedrock rejected the request because its AWS signature has expired. ",
    "Refresh your AWS credentials and retry. If `AWS_BEARER_TOKEN_BEDROCK` is set, ",
    "update or unset it, then restart Codex",
);

pub(super) fn map_api_error(error: ApiError) -> CodexErr {
    let expired_signature = matches!(
        &error,
        ApiError::Transport(TransportError::Http {
            status: StatusCode::UNAUTHORIZED,
            body: Some(body),
            ..
        }) if body.contains("Signature expired:")
    );
    let error = codex_api::map_api_error(error);
    if !expired_signature {
        return error;
    }
    let CodexErrorDetails::UnexpectedStatus(response) = error.details() else {
        return error;
    };
    if response.status != StatusCode::UNAUTHORIZED {
        return error;
    }
    let mut response = response.clone();
    response.user_message = Some(BEDROCK_EXPIRED_SIGNATURE_MESSAGE.to_string());
    let mapped = CodexErr::new(CodexErrorDetails::UnexpectedStatus(response));
    match error.retry_delay() {
        Some(retry_delay) => mapped.with_retry_delay(retry_delay),
        None => mapped,
    }
}

pub(super) fn is_refreshable_auth_error(error: &TransportError) -> bool {
    match error {
        TransportError::Build(message) | TransportError::Network(message) => {
            message.starts_with("failed to load AWS credentials:")
        }
        TransportError::Http { status, .. } if *status == StatusCode::UNAUTHORIZED => true,
        TransportError::Http {
            status,
            body: Some(body),
            ..
        } if *status == StatusCode::FORBIDDEN => {
            let body = body.to_ascii_lowercase();
            [
                "expiredtoken",
                "unrecognizedclientexception",
                "invalidclienttokenid",
            ]
            .iter()
            .any(|error_code| body.contains(error_code))
        }
        _ => false,
    }
}
