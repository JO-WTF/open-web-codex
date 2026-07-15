use axum::http::StatusCode;
use axum::Json;
use open_web_codex_platform_contracts::error::PlatformError;
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub type AccessResult<T> = Result<T, (StatusCode, Json<PlatformError>)>;

pub struct TaskAccess {
    pub task_id: Uuid,
    pub project_id: Uuid,
    pub organization_id: Uuid,
}

pub struct RunAccess {
    pub run_id: Uuid,
    pub task_id: Uuid,
    pub project_id: Uuid,
    pub organization_id: Uuid,
}

pub async fn default_organization_for_user(
    db: &PgPool,
    user_id: Uuid,
) -> AccessResult<Uuid> {
    let org_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT organization_id
         FROM memberships
         WHERE user_id = $1
         ORDER BY created_at ASC
         LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(db_error)?;

    org_id.ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            Json(PlatformError::forbidden("user does not belong to an organization")),
        )
    })
}

pub async fn ensure_organization_member(
    db: &PgPool,
    user_id: Uuid,
    organization_id: Uuid,
) -> AccessResult<()> {
    let allowed = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM memberships
            WHERE organization_id = $1 AND user_id = $2
         )",
    )
    .bind(organization_id)
    .bind(user_id)
    .fetch_one(db)
    .await
    .map_err(db_error)?;

    if allowed {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(PlatformError::forbidden("organization access denied")),
        ))
    }
}

pub async fn ensure_project_access(
    db: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
) -> AccessResult<Uuid> {
    let row = sqlx::query(
        "SELECT p.organization_id
         FROM projects p
         JOIN memberships m
           ON m.organization_id = p.organization_id
          AND m.user_id = $1
         WHERE p.id = $2",
    )
    .bind(user_id)
    .bind(project_id)
    .fetch_optional(db)
    .await
    .map_err(db_error)?;

    let Some(row) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("project not found")),
        ));
    };

    Ok(row.get("organization_id"))
}

pub async fn ensure_task_access(db: &PgPool, user_id: Uuid, task_id: Uuid) -> AccessResult<TaskAccess> {
    let row = sqlx::query(
        "SELECT t.id AS task_id, t.project_id, p.organization_id
         FROM tasks t
         JOIN projects p ON p.id = t.project_id
         JOIN memberships m
           ON m.organization_id = p.organization_id
          AND m.user_id = $1
         WHERE t.id = $2",
    )
    .bind(user_id)
    .bind(task_id)
    .fetch_optional(db)
    .await
    .map_err(db_error)?;

    let Some(row) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("task not found")),
        ));
    };

    Ok(TaskAccess {
        task_id: row.get("task_id"),
        project_id: row.get("project_id"),
        organization_id: row.get("organization_id"),
    })
}

pub async fn ensure_run_access(db: &PgPool, user_id: Uuid, run_id: Uuid) -> AccessResult<RunAccess> {
    let row = sqlx::query(
        "SELECT r.id AS run_id, r.task_id, t.project_id, p.organization_id
         FROM runs r
         JOIN tasks t ON t.id = r.task_id
         JOIN projects p ON p.id = t.project_id
         JOIN memberships m
           ON m.organization_id = p.organization_id
          AND m.user_id = $1
         WHERE r.id = $2",
    )
    .bind(user_id)
    .bind(run_id)
    .fetch_optional(db)
    .await
    .map_err(db_error)?;

    let Some(row) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("run not found")),
        ));
    };

    Ok(RunAccess {
        run_id: row.get("run_id"),
        task_id: row.get("task_id"),
        project_id: row.get("project_id"),
        organization_id: row.get("organization_id"),
    })
}

pub async fn ensure_approval_access(
    db: &PgPool,
    user_id: Uuid,
    approval_id: Uuid,
) -> AccessResult<RunAccess> {
    let row = sqlx::query(
        "SELECT r.id AS run_id, r.task_id, t.project_id, p.organization_id
         FROM approvals a
         JOIN runs r ON r.id = a.run_id
         JOIN tasks t ON t.id = r.task_id
         JOIN projects p ON p.id = t.project_id
         JOIN memberships m
           ON m.organization_id = p.organization_id
          AND m.user_id = $1
         WHERE a.id = $2",
    )
    .bind(user_id)
    .bind(approval_id)
    .fetch_optional(db)
    .await
    .map_err(db_error)?;

    let Some(row) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("approval not found")),
        ));
    };

    Ok(RunAccess {
        run_id: row.get("run_id"),
        task_id: row.get("task_id"),
        project_id: row.get("project_id"),
        organization_id: row.get("organization_id"),
    })
}

fn db_error(error: sqlx::Error) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(format!("{error}"))),
    )
}
