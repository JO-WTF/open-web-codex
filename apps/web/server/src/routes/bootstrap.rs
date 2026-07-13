use axum::{extract::State, http::StatusCode, Json};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{BootstrapRequest, BootstrapResponse, User};
use open_web_codex_platform_store::AppState;
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// POST /api/bootstrap
///
/// One-time initial setup. Creates the first owner user and returns a session.
/// Fails with 409 if any user already exists.
pub async fn bootstrap(
    State(state): State<AppState>,
    Json(req): Json<BootstrapRequest>,
) -> ApiResult<BootstrapResponse> {
    // Enforce one-time bootstrap
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(format!("db error: {e}"))),
            )
        })?;

    if count > 0 {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request("bootstrap already completed")),
        ));
    }

    if req.name.trim().is_empty() || req.email.trim().is_empty() || req.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("name, email, and password are required")),
        ));
    }

    // Hash password
    let mut hasher = Sha256::new();
    hasher.update(req.password.as_bytes());
    let password_hash = hex::encode(hasher.finalize());

    // Create owner user
    let user = sqlx::query(
        "INSERT INTO users (name, email, password_hash, role) VALUES ($1, $2, $3, 'owner') 
         RETURNING id, name, email, role, created_at, updated_at",
    )
    .bind(&req.name)
    .bind(&req.email)
    .bind(&password_hash)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
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


    // Create default organization
    let org_name = format!("{}'s Organization", req.name);
    let org = sqlx::query("INSERT INTO organizations (name, slug) VALUES ($1, $2) RETURNING id, name, slug, created_at, updated_at")
        .bind(&org_name)
        .bind(&org_name.to_lowercase().replace(' ', "-"))
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(format!("{e}"))))
        })?;

    // Add owner as member
    sqlx::query("INSERT INTO memberships (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org.get::<Uuid, _>("id"))
        .bind(user.get::<Uuid, _>("id"))
        .execute(&state.db)
        .await
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(format!("{e}"))))
        })?;

    sqlx::query(
        "INSERT INTO sessions (user_id, token_hash, expires_at) 
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

    let org_data = open_web_codex_platform_contracts::Organization {
        id: org.get("id"),
        name: org.get("name"),
        slug: org.get("slug"),
        created_at: org.get("created_at"),
        updated_at: org.get("updated_at"),
    };

    Ok(Json(BootstrapResponse {
        user: user_data,
        session_token,
        organization: org_data,
    }))
}
