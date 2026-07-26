use chrono::Utc;
use open_web_codex_adapter::AuthorizedWorkspace;
use sqlx::Row;
use uuid::Uuid;

use crate::{RunLease, RunOrchestrator, RunOrchestratorError};

impl RunOrchestrator {
    pub async fn run_once(&self) -> Result<bool, RunOrchestratorError> {
        let Some(lease) = self.claim_next().await? else {
            return Ok(false);
        };
        if let Err(error) = self.execute_lease(&lease).await {
            self.fail_lease(&lease, failure_code(&error)).await?;
            return Err(error);
        }
        Ok(true)
    }

    pub async fn execute_lease(&self, lease: &RunLease) -> Result<(), RunOrchestratorError> {
        self.heartbeat(lease).await?;
        let workspace = AuthorizedWorkspace {
            id: lease.workspace_id.to_string(),
            root: lease.workspace_root.clone(),
        };
        let started = if let Some(source_thread_id) = lease.fork_thread_id.as_deref() {
            let source = sqlx::query(
                "SELECT source_workspace.id AS workspace_id, source_workspace.root_path \
                 FROM runs source_run \
                 JOIN workspaces source_workspace ON source_workspace.id = source_run.workspace_id \
                 JOIN workspace_grants source_grant \
                   ON source_grant.workspace_id = source_workspace.id \
                  AND source_grant.organization_id = source_workspace.organization_id \
                  AND source_grant.user_id = $4 \
                  AND source_grant.profile_id = source_workspace.profile_id \
                 WHERE source_run.id = $1 AND source_run.organization_id = $2 \
                   AND source_run.codex_thread_id = $3 \
                   AND source_workspace.state IN ('ready', 'retained')",
            )
            .bind(lease.fork_source_run_id)
            .bind(lease.organization_id)
            .bind(source_thread_id)
            .bind(lease.actor_id)
            .fetch_optional(&self.db)
            .await?
            .ok_or(RunOrchestratorError::NotFound)?;
            let source_workspace = AuthorizedWorkspace {
                id: source.get::<Uuid, _>("workspace_id").to_string(),
                root: source.get::<String, _>("root_path").into(),
            };
            self.adapter
                .fork_thread(&source_workspace, &workspace, source_thread_id)
                .await?
        } else {
            self.adapter.start_thread(&workspace).await?
        };

        let updated = sqlx::query(
            "WITH updated_run AS ( \
                 UPDATE runs SET status = 'running', codex_thread_id = $1, heartbeat_at = now(), \
                                 lease_expires_at = $2, updated_at = now() \
                 WHERE id = $3 AND workspace_id = $4 \
                   AND lease_owner = $5 AND lease_token = $6 AND status = 'provisioning' \
                 RETURNING task_id \
             ) \
             UPDATE tasks SET status = 'running', updated_at = now() \
             WHERE id IN (SELECT task_id FROM updated_run)",
        )
        .bind(&started.thread_id)
        .bind(Utc::now() + crate::chrono_ttl(self.lease_ttl)?)
        .bind(lease.run_id)
        .bind(lease.workspace_id)
        .bind(&self.worker_id)
        .bind(&lease.token)
        .execute(&self.db)
        .await?
        .rows_affected();
        if updated != 1 {
            self.record_delivery_uncertainty(lease, &started.thread_id)
                .await?;
            return Err(RunOrchestratorError::LeaseLost);
        }
        Ok(())
    }

    async fn record_delivery_uncertainty(
        &self,
        lease: &RunLease,
        thread_id: &str,
    ) -> Result<(), RunOrchestratorError> {
        sqlx::query(
            "UPDATE runs SET codex_thread_id = COALESCE(codex_thread_id, $1), \
                             status = CASE WHEN status = 'cancelled' THEN 'cancelled' \
                                           ELSE 'recovery_pending' END, \
                             failure_code = CASE WHEN status = 'cancelled' THEN failure_code \
                                                 ELSE 'thread_delivery_unknown' END, \
                             lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, \
                             updated_at = now() \
             WHERE id = $2 AND workspace_id = $3",
        )
        .bind(thread_id)
        .bind(lease.run_id)
        .bind(lease.workspace_id)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn fail_lease(
        &self,
        lease: &RunLease,
        code: &'static str,
    ) -> Result<(), RunOrchestratorError> {
        sqlx::query(
            "UPDATE runs SET status = 'failed', failure_code = $1, lease_owner = NULL, \
                             lease_token = NULL, lease_expires_at = NULL, updated_at = now() \
             WHERE id = $2 AND lease_owner = $3 AND lease_token = $4 \
               AND status IN ('provisioning', 'cancelling')",
        )
        .bind(code)
        .bind(lease.run_id)
        .bind(&self.worker_id)
        .bind(&lease.token)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn run_cleanup_once(&self) -> Result<bool, RunOrchestratorError> {
        let mut transaction = self.db.begin().await?;
        let job = sqlx::query(
            "SELECT job.id, job.workspace_id \
             FROM runner_jobs job \
             WHERE job.kind = 'workspace_cleanup' \
               AND (job.state = 'pending' OR (job.state = 'running' AND job.lease_expires_at < now())) \
               AND job.run_after <= now() \
             ORDER BY job.created_at, job.id FOR UPDATE SKIP LOCKED LIMIT 1",
        )
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(job) = job else {
            transaction.commit().await?;
            return Ok(false);
        };
        let job_id: Uuid = job.get("id");
        let workspace_id: Uuid = job.get("workspace_id");
        let token = Uuid::now_v7().to_string();
        sqlx::query(
            "UPDATE runner_jobs SET state = 'running', attempt = attempt + 1, lease_owner = $1, \
                                    lease_token = $2, lease_expires_at = now() + interval '1 minute', \
                                    updated_at = now() WHERE id = $3",
        )
        .bind(&self.worker_id)
        .bind(&token)
        .bind(job_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;

        if let Err(error) = self.git.remove_workspace(workspace_id).await {
            let state: String = sqlx::query_scalar(
                "UPDATE runner_jobs \
                 SET state = CASE WHEN attempt >= 5 THEN 'failed' ELSE 'pending' END, \
                     run_after = now() + interval '30 seconds', lease_owner = NULL, \
                     lease_token = NULL, lease_expires_at = NULL, \
                     last_error_code = 'workspace_cleanup_failed', updated_at = now() \
                 WHERE id = $1 AND lease_token = $2 RETURNING state",
            )
            .bind(job_id)
            .bind(&token)
            .fetch_one(&self.db)
            .await?;
            if state == "failed" {
                sqlx::query(
                    "UPDATE workspaces SET state = 'cleanup_failed', updated_at = now() \
                     WHERE id = $1 AND state = 'removing'",
                )
                .bind(workspace_id)
                .execute(&self.db)
                .await?;
            }
            return Err(error.into());
        }
        let mut transaction = self.db.begin().await?;
        sqlx::query(
            "UPDATE workspaces \
             SET state = 'removed', removed_at = now(), updated_at = now() WHERE id = $1",
        )
        .bind(workspace_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE runner_jobs SET state = 'completed', lease_owner = NULL, lease_token = NULL, \
                                    lease_expires_at = NULL, updated_at = now() \
             WHERE id = $1 AND lease_token = $2",
        )
        .bind(job_id)
        .bind(&token)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(true)
    }
}

fn failure_code(error: &RunOrchestratorError) -> &'static str {
    match error {
        RunOrchestratorError::Invalid(_) => "invalid_run",
        RunOrchestratorError::NotFound => "resource_not_found",
        RunOrchestratorError::Conflict(_) => "run_conflict",
        RunOrchestratorError::LeaseLost => "lease_lost",
        RunOrchestratorError::Database(_) => "database_error",
        RunOrchestratorError::Git(_) => "git_workspace_error",
        RunOrchestratorError::Adapter(_) => "codex_unavailable",
    }
}
