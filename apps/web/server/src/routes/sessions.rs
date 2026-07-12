use axum::{extract::State, http::StatusCode, Json};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{LoginRequest, LoginResponse, User};
use open_web_codex_platform_store::AppState;
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// POST /api/sessions — Login with email + password.
pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<LoginResponse> {
    // Hash password
    let mut hasher = Sha256::new();
    hasher.update(req.password.as_bytes());
    let password_hash = hex::encode(hasher.finalize());

    // Find user
    let user = sqlx::query(
        "SELECT id, name, email, role, created_at, updated_at \
         FROM users WHERE email = $1 AND password_hash = $2",
    )
    .bind(&req.email)
    .bind(&password_hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
        )
    })?;

    let user = user.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(PlatformError::unauthorized("invalid email or password")),
        )
    })?;

    // Generate session token
    let session_token: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(64)
        .map(char::from)
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(session_token.as_bytes());
    let token_hash = hex::encode(hasher.finalize());

    sqlx::query(
        "INSERT INTO sessions (user_id, token_hash, expires_at) \
         VALUES ($1, $2, now() + interval '7 days')",
    )
    .bind(user.get::<Uuid, _>("id"))
    .bind(&token_hash)
    .execute(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
        )
    })?;

    let user_data = User {
        id: user.get("id"),
        name: user.get("name"),
        email: user.get("email"),
        role: user.get("role"),
        created_at: user.get("created_at"),
        updated_at: user.get("updated_at"),
    };

    Ok(Json(LoginResponse {
        user: user_data,
        session_token,
    }))
}
