use axum::{extract::State, http::StatusCode, Extension, Json};
use open_web_codex_auth::hash_password;
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{BootstrapRequest, BootstrapResponse, User};
use open_web_codex_platform_store::AppState;
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use super::RuntimeProfileBinding;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// POST /api/bootstrap
///
/// One-time initial setup. Creates the first owner user and returns a session.
/// Fails with 409 if any user already exists.
pub async fn bootstrap(
    State(state): State<AppState>,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Json(req): Json<BootstrapRequest>,
) -> ApiResult<BootstrapResponse> {
    if req.name.trim().is_empty() || req.email.trim().is_empty() || req.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "name, email, and password are required",
            )),
        ));
    }

    let password = req.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|_| internal_password_error())?
        .map_err(|_| internal_password_error())?;

    let mut transaction = state.db.begin().await.map_err(internal_database_error)?;

    // Serialize bootstrap attempts so two requests cannot both create an owner.
    sqlx::query("LOCK TABLE users IN EXCLUSIVE MODE")
        .execute(&mut *transaction)
        .await
        .map_err(internal_database_error)?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&mut *transaction)
        .await
        .map_err(internal_database_error)?;

    if count > 0 {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request("bootstrap already completed")),
        ));
    }

    // Create owner user
    let user = sqlx::query(
        "INSERT INTO users (name, email, password_hash, role) VALUES ($1, $2, $3, 'owner') 
         RETURNING id, name, email, role, created_at, updated_at",
    )
    .bind(&req.name)
    .bind(&req.email)
    .bind(&password_hash)
    .fetch_one(&mut *transaction)
    .await
    .map_err(internal_database_error)?;

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
        .fetch_one(&mut *transaction)
        .await
        .map_err(internal_database_error)?;

    // Add owner as member
    sqlx::query(
        "INSERT INTO memberships (organization_id, user_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(org.get::<Uuid, _>("id"))
    .bind(user.get::<Uuid, _>("id"))
    .execute(&mut *transaction)
    .await
    .map_err(internal_database_error)?;

    sqlx::query(
        "INSERT INTO profiles (organization_id, owner_user_id, runtime_key, name) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(org.get::<Uuid, _>("id"))
    .bind(user.get::<Uuid, _>("id"))
    .bind(&profile.runtime_key)
    .bind(&profile.name)
    .execute(&mut *transaction)
    .await
    .map_err(internal_database_error)?;

    if let Some(capabilities) = profile.capabilities.get().await {
        sqlx::query(
            "INSERT INTO profile_capabilities \
             (profile_id, server_build, protocol_version, manifest, observed_at) \
             SELECT id, $1, $2, $3, now() FROM profiles WHERE runtime_key = $4 \
             ON CONFLICT (profile_id) DO UPDATE SET server_build = EXCLUDED.server_build, \
             protocol_version = EXCLUDED.protocol_version, manifest = EXCLUDED.manifest, \
             observed_at = now()",
        )
        .bind(capabilities.server_build)
        .bind(capabilities.protocol_version)
        .bind(capabilities.manifest)
        .bind(&profile.runtime_key)
        .execute(&mut *transaction)
        .await
        .map_err(internal_database_error)?;
    }

    sqlx::query(
        "INSERT INTO sessions (user_id, organization_id, token_hash, expires_at) \
         VALUES ($1, $2, $3, now() + interval '7 days')",
    )
    .bind(user.get::<Uuid, _>("id"))
    .bind(org.get::<Uuid, _>("id"))
    .bind(&token_hash)
    .execute(&mut *transaction)
    .await
    .map_err(internal_database_error)?;

    transaction
        .commit()
        .await
        .map_err(internal_database_error)?;

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

fn internal_database_error(_error: sqlx::Error) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("database operation failed")),
    )
}

fn internal_password_error() -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("password hashing failed")),
    )
}
