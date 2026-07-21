use axum::{extract::State, http::StatusCode, Json};
use open_web_codex_auth::{hash_password, needs_rehash, verify_password_or_dummy};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    LoginRequest, LoginResponse, Organization, SelectOrganizationRequest, SessionOrganization, User,
};
use open_web_codex_platform_store::AppState;
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// POST /api/sessions — Login with email + password into one Organization.
pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<LoginResponse> {
    let user = sqlx::query(
        "SELECT id, name, email, role, password_hash, created_at, updated_at \
         FROM users WHERE email = $1",
    )
    .bind(&req.email)
    .fetch_optional(&state.db)
    .await
    .map_err(internal_database_error)?;

    let encoded = user
        .as_ref()
        .map(|row| row.get::<String, _>("password_hash"));
    let password = req.password.clone();
    let valid = tokio::task::spawn_blocking(move || {
        verify_password_or_dummy(&password, encoded.as_deref())
    })
    .await
    .map_err(|_| internal_password_error())?;
    if !valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(PlatformError::unauthorized("invalid email or password")),
        ));
    }
    let user = user.expect("password verification requires an existing user");
    let user_id: Uuid = user.get("id");

    let organization = sqlx::query(
        "SELECT o.id, o.name, o.slug, o.created_at, o.updated_at, m.role AS membership_role \
         FROM memberships m JOIN organizations o ON o.id = m.organization_id \
         WHERE m.user_id = $1 AND ($2::uuid IS NULL OR o.id = $2) \
         ORDER BY m.created_at, m.id LIMIT 1",
    )
    .bind(user_id)
    .bind(req.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(internal_database_error)?
    .ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            Json(PlatformError::forbidden(
                "user is not a member of the requested Organization",
            )),
        )
    })?;

    let existing_hash: String = user.get("password_hash");
    if needs_rehash(&existing_hash) {
        let password = req.password.clone();
        let replacement = tokio::task::spawn_blocking(move || hash_password(&password))
            .await
            .map_err(|_| internal_password_error())?
            .map_err(|_| internal_password_error())?;
        sqlx::query("UPDATE users SET password_hash = $1, updated_at = now() WHERE id = $2")
            .bind(replacement)
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(internal_database_error)?;
    }

    let session_token: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(64)
        .map(char::from)
        .collect();
    let token_hash = hex::encode(Sha256::digest(session_token.as_bytes()));

    sqlx::query(
        "INSERT INTO sessions (user_id, organization_id, token_hash, expires_at) \
         VALUES ($1, $2, $3, now() + interval '7 days')",
    )
    .bind(user_id)
    .bind(organization.get::<Uuid, _>("id"))
    .bind(&token_hash)
    .execute(&state.db)
    .await
    .map_err(internal_database_error)?;

    Ok(Json(LoginResponse {
        user: User {
            id: user_id,
            name: user.get("name"),
            email: user.get("email"),
            role: user.get("role"),
            created_at: user.get("created_at"),
            updated_at: user.get("updated_at"),
        },
        organization: organization_from_row(&organization),
        membership_role: organization.get("membership_role"),
        session_token,
    }))
}

/// PUT /api/sessions/organization — change only the current session's
/// Organization after proving membership.
pub async fn select_organization(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<SelectOrganizationRequest>,
) -> ApiResult<SessionOrganization> {
    let organization = sqlx::query(
        "SELECT o.id, o.name, o.slug, o.created_at, o.updated_at, m.role AS membership_role \
         FROM memberships m JOIN organizations o ON o.id = m.organization_id \
         WHERE m.user_id = $1 AND m.organization_id = $2",
    )
    .bind(auth.user_id)
    .bind(req.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(internal_database_error)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Organization was not found")),
        )
    })?;

    let updated = sqlx::query(
        "UPDATE sessions SET organization_id = $1 \
         WHERE id = $2 AND user_id = $3 AND revoked_at IS NULL",
    )
    .bind(req.organization_id)
    .bind(auth.session_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await
    .map_err(internal_database_error)?;
    if updated.rows_affected() != 1 {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(PlatformError::unauthorized("session is no longer active")),
        ));
    }

    Ok(Json(SessionOrganization {
        organization: organization_from_row(&organization),
        role: organization.get("membership_role"),
    }))
}

fn organization_from_row(row: &sqlx::postgres::PgRow) -> Organization {
    Organization {
        id: row.get("id"),
        name: row.get("name"),
        slug: row.get("slug"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
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
        Json(PlatformError::internal("password verification failed")),
    )
}
