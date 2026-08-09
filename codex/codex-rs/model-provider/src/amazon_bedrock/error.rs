use codex_api::ApiError;
use codex_protocol::error::CodexErr;

pub(super) fn map_api_error(error: ApiError) -> CodexErr {
    codex_api::map_api_error(error)
}
