use codex_api::ApiError;
use codex_api::TransportError;
use codex_protocol::error::CodexErr;

pub(super) fn map_api_error(error: ApiError) -> CodexErr {
    codex_api::map_api_error(error)
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
