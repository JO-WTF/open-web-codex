use sqlx::Row;
use uuid::Uuid;

use crate::{
    validate_idempotency_key, CreateWorkspaceRequest, RemoveWorkspaceRequest, RunOrchestrator,
    RunOrchestratorError, WorkspaceRecord,
};

impl RunOrchestrator {
    pub async fn create_workspace(
        &self,
        request: CreateWorkspaceRequest,
    ) -> Result<WorkspaceRecord, RunOrchestratorError> {
        validate_idempotency_key(&request.idempotency_key)?;
        if !matches!(request.kind.as_str(), "main" | "worktree" | "clone") {
            return Err(RunOrchestratorError::Invalid(
                "workspace kind is invalid".to_string(),
            ));
        }
        if request.kind == "main" && request.parent_workspace_id.is_some() {
            return Err(RunOrchestratorError::Invalid(
                "main Workspaces cannot have a parent".to_string(),
            ));
        }
        if request.kind == "worktree" && request.parent_workspace_id.is_none() {
            return Err(RunOrchestratorError::Invalid(
                "worktree Workspaces require a parent Workspace".to_string(),
            ));
        }
        let name = request
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if name.as_ref().is_some_and(|value| value.len() > 128) {
            return Err(RunOrchestratorError::Invalid(
                "workspace name is too long".to_string(),
            ));
        }

        let project = sqlx::query(
            "SELECT project.name, project.git_url, project.default_branch, profile.id AS profile_id \
             FROM projects project \
             JOIN profiles profile ON profile.organization_id = project.organization_id \
               AND profile.owner_user_id = $2 AND profile.runtime_key = $4 \
               AND profile.status = 'active' \
             WHERE project.id = $1 AND project.organization_id = $3",
        )
        .bind(request.project_id)
        .bind(request.actor_id)
        .bind(request.organization_id)
        .bind(&self.runtime_key)
        .fetch_optional(&self.db)
        .await?
        .ok_or(RunOrchestratorError::NotFound)?;
        let git_url: String = project.get("git_url");
        let source = self.git.validate_source(&git_url)?;
        let source_ref = request
            .source_ref
            .unwrap_or_else(|| project.get::<String, _>("default_branch"));
        let git_ref = self.git.validate_ref(&source_ref)?;
        let profile_id: Uuid = project.get("profile_id");

        if let Some(existing) = sqlx::query(
            "SELECT id, project_id, profile_id, name, kind, state, source_ref, branch_name, \
                    parent_workspace_id, group_workspace_id, managed, created_at, updated_at \
             FROM workspaces \
             WHERE organization_id = $1 AND created_by = $2 AND idempotency_key = $3",
        )
        .bind(request.organization_id)
        .bind(request.actor_id)
        .bind(&request.idempotency_key)
        .fetch_optional(&self.db)
        .await?
        {
            let record = workspace_record(&existing);
            if record.project_id != request.project_id
                || record.profile_id != profile_id
                || record.kind != request.kind
                || record.source_ref != source_ref
                || record.parent_workspace_id != request.parent_workspace_id
            {
                return Err(RunOrchestratorError::Conflict(
                    "idempotency key was already used for a different Workspace request"
                        .to_string(),
                ));
            }
            return if record.state == "ready" || record.state == "retained" {
                Ok(record)
            } else {
                Err(RunOrchestratorError::Conflict(format!(
                    "Workspace creation is in state '{}'",
                    record.state
                )))
            };
        }

        let mut transaction = self.db.begin().await?;
        let (parent_workspace_id, group_workspace_id) =
            if let Some(parent_workspace_id) = request.parent_workspace_id {
                let parent = sqlx::query(
                    "SELECT workspace.id, workspace.group_workspace_id \
                     FROM workspaces workspace \
                     JOIN workspace_grants grant ON grant.workspace_id = workspace.id \
                       AND grant.organization_id = workspace.organization_id \
                     WHERE workspace.id = $1 AND workspace.organization_id = $2 \
                       AND workspace.project_id = $3 AND workspace.profile_id = $4 \
                       AND grant.user_id = $5 AND grant.profile_id = $4 \
                       AND grant.role IN ('owner', 'write') \
                       AND workspace.state IN ('ready', 'retained') \
                     FOR KEY SHARE OF workspace",
                )
                .bind(parent_workspace_id)
                .bind(request.organization_id)
                .bind(request.project_id)
                .bind(profile_id)
                .bind(request.actor_id)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or(RunOrchestratorError::NotFound)?;
                (
                    Some(parent_workspace_id),
                    Some(
                        parent
                            .get::<Option<Uuid>, _>("group_workspace_id")
                            .unwrap_or(parent_workspace_id),
                    ),
                )
            } else {
                (None, None)
            };

        let workspace_id = Uuid::now_v7();
        let workspace_name = name.unwrap_or_else(|| {
            if request.kind == "main" {
                project.get("name")
            } else {
                source_ref.clone()
            }
        });
        let root = self.git.workspace_path(workspace_id);
        let inserted = sqlx::query(
            "INSERT INTO workspaces \
             (id, organization_id, project_id, profile_id, created_by, root_path, state, \
              source_ref, kind, name, parent_workspace_id, group_workspace_id, managed, \
              idempotency_key) \
             VALUES ($1, $2, $3, $4, $5, $6, 'creating', $7, $8, $9, $10, $11, TRUE, $12) \
             ON CONFLICT (organization_id, created_by, idempotency_key) \
               WHERE idempotency_key IS NOT NULL DO NOTHING",
        )
        .bind(workspace_id)
        .bind(request.organization_id)
        .bind(request.project_id)
        .bind(profile_id)
        .bind(request.actor_id)
        .bind(root.to_string_lossy().as_ref())
        .bind(&source_ref)
        .bind(&request.kind)
        .bind(&workspace_name)
        .bind(parent_workspace_id)
        .bind(group_workspace_id)
        .bind(&request.idempotency_key)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        if inserted != 1 {
            transaction.rollback().await?;
            return Err(RunOrchestratorError::Conflict(
                "Workspace creation is already in progress".to_string(),
            ));
        }
        sqlx::query(
            "INSERT INTO workspace_grants \
             (workspace_id, organization_id, user_id, profile_id, role) \
             VALUES ($1, $2, $3, $4, 'owner')",
        )
        .bind(workspace_id)
        .bind(request.organization_id)
        .bind(request.actor_id)
        .bind(profile_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;

        let provisioned = async {
            let mut checkout = self
                .git
                .provision(request.project_id, workspace_id, &source, &git_ref)
                .await?;
            if request.kind == "worktree" {
                self.git
                    .switch_or_create_branch(workspace_id, &source_ref)
                    .await?;
                checkout.branch = source_ref.clone();
            }
            if request.copy_agents_md {
                if let Some(parent_workspace_id) = parent_workspace_id {
                    self.git
                        .copy_agents_md(parent_workspace_id, workspace_id)
                        .await?;
                }
            }
            Ok::<_, open_web_codex_git_runtime::GitRuntimeError>(checkout)
        }
        .await;

        let checkout = match provisioned {
            Ok(checkout) => checkout,
            Err(error) => {
                let _ = self.git.remove_workspace(workspace_id).await;
                sqlx::query(
                    "UPDATE workspaces SET state = 'cleanup_failed', updated_at = now() \
                     WHERE id = $1 AND state = 'creating'",
                )
                .bind(workspace_id)
                .execute(&self.db)
                .await?;
                return Err(error.into());
            }
        };
        sqlx::query(
            "UPDATE workspaces \
             SET state = 'ready', head_commit = $1, branch_name = $2, updated_at = now() \
             WHERE id = $3 AND state = 'creating'",
        )
        .bind(&checkout.head_commit)
        .bind(&checkout.branch)
        .bind(workspace_id)
        .execute(&self.db)
        .await?;
        self.get_workspace(request.organization_id, workspace_id)
            .await
    }

    pub async fn get_workspace(
        &self,
        organization_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<WorkspaceRecord, RunOrchestratorError> {
        let row = sqlx::query(
            "SELECT id, project_id, profile_id, name, kind, state, source_ref, branch_name, \
                    parent_workspace_id, group_workspace_id, managed, created_at, updated_at \
             FROM workspaces WHERE id = $1 AND organization_id = $2",
        )
        .bind(workspace_id)
        .bind(organization_id)
        .fetch_optional(&self.db)
        .await?
        .ok_or(RunOrchestratorError::NotFound)?;
        Ok(workspace_record(&row))
    }

    pub async fn remove_workspace(
        &self,
        request: RemoveWorkspaceRequest,
    ) -> Result<WorkspaceRecord, RunOrchestratorError> {
        let mut transaction = self.db.begin().await?;
        let row = sqlx::query(
            "SELECT workspace.id, workspace.project_id, workspace.profile_id, workspace.name, \
                    workspace.kind, workspace.state, workspace.source_ref, workspace.branch_name, \
                    workspace.parent_workspace_id, workspace.group_workspace_id, workspace.managed, \
                    workspace.created_at, workspace.updated_at, workspace.created_by, \
                    EXISTS(SELECT 1 FROM workspace_grants grant \
                           WHERE grant.workspace_id = workspace.id AND grant.user_id = $3 \
                             AND grant.profile_id = workspace.profile_id \
                             AND grant.role = 'owner') AS owns_workspace \
             FROM workspaces workspace \
             WHERE workspace.id = $1 AND workspace.organization_id = $2 \
             FOR UPDATE OF workspace",
        )
        .bind(request.workspace_id)
        .bind(request.organization_id)
        .bind(request.actor_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(RunOrchestratorError::NotFound)?;
        if !row.get::<bool, _>("owns_workspace") && !request.allow_organization_admin {
            return Err(RunOrchestratorError::NotFound);
        }
        let record = workspace_record(&row);
        if record.state == "removed" {
            transaction.commit().await?;
            return Ok(record);
        }
        if record.state == "removing" {
            transaction.commit().await?;
            return Ok(record);
        }
        if !record.managed {
            return Err(RunOrchestratorError::Conflict(
                "registered Workspace roots are removed by revoking their grant".to_string(),
            ));
        }
        let active_runs: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM runs \
             WHERE workspace_id = $1 \
               AND status IN ('pending', 'provisioning', 'running', 'cancelling', 'recovery_pending'))",
        )
        .bind(request.workspace_id)
        .fetch_one(&mut *transaction)
        .await?;
        if active_runs {
            return Err(RunOrchestratorError::Conflict(
                "Workspace is still used by an active Run".to_string(),
            ));
        }
        let active_children: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspaces \
             WHERE parent_workspace_id = $1 AND state <> 'removed')",
        )
        .bind(request.workspace_id)
        .fetch_one(&mut *transaction)
        .await?;
        if active_children {
            return Err(RunOrchestratorError::Conflict(
                "remove child Workspaces before removing their parent".to_string(),
            ));
        }
        let active_terminals: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM terminal_sessions \
             WHERE workspace_id = $1 AND state IN ('starting', 'running', 'closing'))",
        )
        .bind(request.workspace_id)
        .fetch_one(&mut *transaction)
        .await?;
        if active_terminals {
            return Err(RunOrchestratorError::Conflict(
                "Workspace still has an active terminal session".to_string(),
            ));
        }
        if record.state != "cleanup_failed"
            && !self
                .git
                .status(request.workspace_id)
                .await?
                .changes
                .is_empty()
        {
            return Err(RunOrchestratorError::Conflict(
                "Workspace has uncommitted changes".to_string(),
            ));
        }

        let updated = sqlx::query(
            "UPDATE workspaces SET state = 'removing', updated_at = now() \
             WHERE id = $1 AND organization_id = $2 \
               AND state IN ('ready', 'retained', 'cleanup_failed')",
        )
        .bind(request.workspace_id)
        .bind(request.organization_id)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(RunOrchestratorError::Conflict(
                "Workspace cannot be removed from its current state".to_string(),
            ));
        }
        sqlx::query(
            "INSERT INTO runner_jobs (organization_id, workspace_id, kind) \
             VALUES ($1, $2, 'workspace_cleanup') \
             ON CONFLICT (workspace_id, kind) DO UPDATE \
             SET state = 'pending', run_after = now(), lease_owner = NULL, \
                 lease_token = NULL, lease_expires_at = NULL, last_error_code = NULL, \
                 updated_at = now()",
        )
        .bind(request.organization_id)
        .bind(request.workspace_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.get_workspace(request.organization_id, request.workspace_id)
            .await
    }
}

pub(crate) fn workspace_record(row: &sqlx::postgres::PgRow) -> WorkspaceRecord {
    WorkspaceRecord {
        id: row.get("id"),
        project_id: row.get("project_id"),
        profile_id: row.get("profile_id"),
        name: row.get("name"),
        kind: row.get("kind"),
        state: row.get("state"),
        source_ref: row.get("source_ref"),
        branch_name: row.get("branch_name"),
        parent_workspace_id: row.get("parent_workspace_id"),
        group_workspace_id: row.get("group_workspace_id"),
        managed: row.get("managed"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
