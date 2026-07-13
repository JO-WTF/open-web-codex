use axum::Json;
use open_web_codex_platform_contracts::MeResponse;
use crate::middleware::auth::AuthenticatedUser;

/// GET /api/me
pub async fn me(
    auth: AuthenticatedUser,
) -> Json<MeResponse> {
    Json(MeResponse {
        id: auth.user_id,
        name: auth.name,
        email: auth.email,
        role: auth.role,
    })
}
