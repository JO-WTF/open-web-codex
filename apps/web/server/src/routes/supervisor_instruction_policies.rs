use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    SupervisorInstructionPolicyDetail, SupervisorInstructionPolicyPublishRequest,
    SupervisorInstructionPolicySelection, SupervisorInstructionPolicySummary,
};
use open_web_codex_platform_store::AppState;
use open_web_codex_supervisor_catalog::instruction_policy;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;
use crate::supervisor_instruction_policy::{self, InstructionPolicyError};

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn list_published(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
) -> ApiResult<Vec<SupervisorInstructionPolicySummary>> {
    supervisor_instruction_policy::list_published(&state.db)
        .await
        .map(Json)
        .map_err(map_policy_error)
}

pub async fn get_published(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path((policy_id, version)): Path<(String, String)>,
) -> ApiResult<SupervisorInstructionPolicyDetail> {
    supervisor_instruction_policy::resolve(
        &state.db,
        &SupervisorInstructionPolicySelection { policy_id, version },
    )
    .await
    .map(Json)
    .map_err(map_policy_error)
}

pub async fn publish(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(request): Json<SupervisorInstructionPolicyPublishRequest>,
) -> ApiResult<SupervisorInstructionPolicyDetail> {
    if auth.role != "owner" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(PlatformError::forbidden(
                "platform owner permission is required",
            )),
        ));
    }
    if supervisor_instruction_policy::is_reserved_builtin_policy_version(
        &request.policy_id,
        &request.version,
    ) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "The policy id is reserved by a built-in platform contract",
            )),
        ));
    }
    let validated = instruction_policy::validate_platform_release(
        request.policy_id,
        request.version,
        request.display_name,
        request.description,
        request.platform_instructions,
    )
    .map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "Supervisor instruction policy fields are invalid",
            )),
        )
    })?;
    let release_id = Uuid::now_v7();
    let mut transaction = state.db.begin().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "Supervisor instruction policy database operation failed",
            )),
        )
    })?;
    let row = sqlx::query(
        "INSERT INTO supervisor_instruction_policy_releases \
         (id, policy_id, version, display_name, description, platform_instructions, \
          content_sha256, published_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING id, policy_id, version, display_name, description, platform_instructions, \
                   content_sha256",
    )
    .bind(release_id)
    .bind(&validated.policy_id)
    .bind(&validated.version)
    .bind(&validated.display_name)
    .bind(&validated.description)
    .bind(&validated.platform_instructions)
    .bind(&validated.content_sha256)
    .bind(auth.user_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| {
        (
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "This instruction policy version is already published",
            )),
        )
    })?;
    sqlx::query(
        "INSERT INTO audit_log (organization_id, actor_id, action, target_type, target_id) \
         VALUES ($1, $2, 'supervisor.instruction_policy.published', \
                 'supervisor_instruction_policy', $3)",
    )
    .bind(auth.organization_id)
    .bind(auth.user_id)
    .bind(release_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "Supervisor instruction policy audit failed",
            )),
        )
    })?;
    let detail = supervisor_instruction_policy::resolve_row(&row).map_err(map_policy_error)?;
    transaction.commit().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "Supervisor instruction policy publication failed",
            )),
        )
    })?;
    Ok(Json(detail))
}

fn map_policy_error(error: InstructionPolicyError) -> ApiError {
    match error {
        InstructionPolicyError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found(
                "Supervisor instruction policy was not found",
            )),
        ),
        InstructionPolicyError::Invalid => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "published Supervisor instruction policy is invalid",
            )),
        ),
        InstructionPolicyError::Database => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "Supervisor instruction policy database operation failed",
            )),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_platform_policy_publication_without_platform_owner_role() {
        let db = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgresql://invalid@127.0.0.1/unused")
            .unwrap();
        let state = AppState::new(db);
        let auth = AuthenticatedUser {
            session_id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            name: "Organization Admin".to_string(),
            username: "organization-admin".to_string(),
            email: "admin@example.invalid".to_string(),
            role: "admin".to_string(),
            organization_id: Uuid::now_v7(),
            organization_role: "owner".to_string(),
        };
        let error = publish(
            State(state),
            auth,
            Json(SupervisorInstructionPolicyPublishRequest {
                policy_id: "platform-supervisor-behavior".to_string(),
                version: "1.1.0".to_string(),
                display_name: "Platform Supervisor behavior".to_string(),
                description: "Updated behavior.".to_string(),
                platform_instructions: "Use authorized Runtime capabilities.".to_string(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(error.0, StatusCode::FORBIDDEN);
    }
}
