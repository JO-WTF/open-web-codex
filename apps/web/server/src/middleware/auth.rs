use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_store::AppState;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

/// Authenticated user extracted from the session token
/// (Authorization: Bearer <token> header or session_token cookie).
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = (StatusCode, Json<PlatformError>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .or_else(|| {
                parts
                    .headers
                    .get("cookie")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|c| {
                        c.split(';')
                            .find(|part| part.trim().starts_with("session_token="))
                            .and_then(|part| part.trim().strip_prefix("session_token="))
                    })
            });

        let token = token.ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(PlatformError::unauthorized("missing session token")),
            )
        })?;

        // Hash the token for lookup
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let token_hash = hex::encode(hasher.finalize());

        let row = sqlx::query(
            "SELECT u.id, u.name, u.email, u.role \
             FROM sessions s JOIN users u ON u.id = s.user_id \
             WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > now()",
        )
        .bind(&token_hash)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(format!("db error: {e}"))),
            )
        })?;

        let row = row.ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(PlatformError::unauthorized("invalid or expired session")),
            )
        })?;

        Ok(AuthenticatedUser {
            user_id: row.get("id"),
            name: row.get("name"),
            email: row.get("email"),
            role: row.get("role"),
        })
    }
}
