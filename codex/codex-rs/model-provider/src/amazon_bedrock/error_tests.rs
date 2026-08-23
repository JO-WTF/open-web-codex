use codex_api::ApiError;
use codex_api::TransportError;
use codex_protocol::error::CodexErrorDetails;
use http::HeaderMap;
use http::HeaderValue;
use http::StatusCode;
use pretty_assertions::assert_eq;

use super::error::BEDROCK_EXPIRED_SIGNATURE_MESSAGE;
use super::error::is_refreshable_auth_error;
use super::error::map_api_error;

const BEDROCK_RESPONSES_URL: &str = "https://bedrock-mantle.us-east-2.api.aws/openai/v1/responses";

fn http_error(status: StatusCode, body: &str) -> ApiError {
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static("req-bedrock"));
    ApiError::Transport(TransportError::Http {
        status,
        url: Some(BEDROCK_RESPONSES_URL.to_string()),
        headers: Some(headers),
        body: Some(body.to_string()),
    })
}

#[test]
fn unauthorized_signature_error_uses_static_guidance_without_response_details() {
    const CANARY: &str = "signature-body-canary";
    let error = map_api_error(http_error(
        StatusCode::UNAUTHORIZED,
        &format!(
            "Signature expired: 20260609T133205Z is now earlier than 20260614T062525Z; {CANARY}"
        ),
    ));

    let CodexErrorDetails::UnexpectedStatus(response) = error.details() else {
        panic!("expected unexpected status error, got {error:?}");
    };
    assert_eq!(
        response.user_message.as_deref(),
        Some(BEDROCK_EXPIRED_SIGNATURE_MESSAGE)
    );
    assert!(response.body.is_empty());
    assert!(response.url.is_none());
    assert_eq!(
        error.to_string(),
        format!("{BEDROCK_EXPIRED_SIGNATURE_MESSAGE}, request id: req-bedrock")
    );
    for rendered in [
        format!("{error:?}"),
        error.to_error_event(/*message_prefix*/ None).message,
    ] {
        assert!(!rendered.contains(CANARY));
        assert!(!rendered.contains(BEDROCK_RESPONSES_URL));
    }
}

#[test]
fn unauthorized_errors_do_not_expose_provider_response_body() {
    const CANARY: &str = "security token";
    let error = map_api_error(http_error(
        StatusCode::UNAUTHORIZED,
        "The security token included in the request is invalid",
    ));

    let CodexErrorDetails::UnexpectedStatus(response) = error.details() else {
        panic!("expected unexpected status error, got {error:?}");
    };
    assert_eq!(
        response.user_message.as_deref(),
        Some("Authentication failed. Check the Provider credentials.")
    );
    assert!(response.body.is_empty());
    assert!(response.url.is_none());
    for rendered in [
        error.to_string(),
        format!("{error:?}"),
        error.to_error_event(/*message_prefix*/ None).message,
    ] {
        assert!(!rendered.contains(CANARY));
        assert!(!rendered.contains(BEDROCK_RESPONSES_URL));
    }
}

#[test]
fn signature_errors_with_other_statuses_remain_generic() {
    let error = map_api_error(http_error(
        StatusCode::FORBIDDEN,
        "Signature expired: old is now earlier than new",
    ));

    let CodexErrorDetails::UnexpectedStatus(response) = error.details() else {
        panic!("expected unexpected status error, got {error:?}");
    };
    assert_eq!(response.user_message, None);
}

#[test]
fn classifies_only_refreshable_bedrock_auth_failures() {
    let cases = [
        (
            StatusCode::UNAUTHORIZED,
            Some("ExpiredTokenException"),
            true,
        ),
        (
            StatusCode::UNAUTHORIZED,
            Some("AccessDeniedException"),
            true,
        ),
        (StatusCode::UNAUTHORIZED, Some(""), true),
        (StatusCode::UNAUTHORIZED, None, true),
        (StatusCode::FORBIDDEN, Some("ExpiredTokenException"), true),
        (
            StatusCode::FORBIDDEN,
            Some("UnrecognizedClientException"),
            true,
        ),
        (StatusCode::FORBIDDEN, Some("InvalidClientTokenId"), true),
        (StatusCode::FORBIDDEN, Some("AccessDeniedException"), false),
        (
            StatusCode::FORBIDDEN,
            Some("The security token included in the request is invalid"),
            false,
        ),
        (
            StatusCode::TOO_MANY_REQUESTS,
            Some("ExpiredTokenException"),
            false,
        ),
        (StatusCode::BAD_REQUEST, Some("RequestExpired"), false),
    ];

    for (status, body, expected) in cases {
        let error = TransportError::Http {
            status,
            url: Some(BEDROCK_RESPONSES_URL.to_string()),
            headers: None,
            body: body.map(str::to_string),
        };
        assert_eq!(
            is_refreshable_auth_error(&error),
            expected,
            "{status}: {body:?}"
        );
    }

    for (error, expected) in [
        (
            TransportError::Build("failed to load AWS credentials: expired".to_string()),
            true,
        ),
        (
            TransportError::Network("failed to load AWS credentials: unavailable".to_string()),
            true,
        ),
        (
            TransportError::Build("request URL is not a valid URI".to_string()),
            false,
        ),
    ] {
        assert_eq!(is_refreshable_auth_error(&error), expected, "{error}");
    }
}
