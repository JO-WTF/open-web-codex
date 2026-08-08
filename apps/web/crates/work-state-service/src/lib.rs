//! Durable platform-owned projection of domain work.
//!
//! Codex owns model-visible Thread, Turn, Agent and Tool semantics. This
//! service owns only explicit domain components, blockers, deliverables and
//! idempotent operations. All large values are immutable resource references.

use open_web_codex_platform_contracts::{
    AssignmentContract, CollaborationContext, PlatformToolResult, PlatformToolResultStatus,
    ReleaseIdentity, WorkBlockingInputSummary, WorkComponentMutation, WorkComponentState,
    WorkComponentSummary, WorkDeliverableSummary, WorkOperationStatus, WorkOperationSummary,
    WorkResourceReference, WorkStateDefinition, WorkStateMutation, WorkStateSummary,
};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum WorkStateServiceError {
    #[error("work state was not found")]
    NotFound,
    #[error("work state revision conflicts with current state")]
    RevisionConflict,
    #[error("work operation conflicts with its current state")]
    OperationConflict,
    #[error("work state contract is invalid: {0}")]
    InvalidContract(String),
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("stored work state data is invalid")]
    InvalidStoredData,
}

#[derive(Debug, Clone)]
pub struct WorkStateActor {
    pub organization_id: Uuid,
    pub profile_id: Uuid,
    pub user_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct CreateWorkStateRequest {
    pub actor: WorkStateActor,
    pub workspace_id: Uuid,
    pub task_id: Uuid,
    pub definition: WorkStateDefinition,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct BeginWorkOperationRequest {
    pub state_id: Uuid,
    pub kind: String,
    pub idempotency_key: String,
    pub input_references: Vec<(String, WorkResourceReference)>,
}

#[derive(Debug, Clone)]
pub struct ApplyWorkMutationRequest {
    pub state_id: Uuid,
    pub operation_id: Uuid,
    pub mutation: WorkStateMutation,
}

/// Platform-owned inputs used to compile one Agent assignment. The caller
/// passes exact Releases and Work State summaries rather than display labels
/// or model-provided capability names.
#[derive(Debug, Clone)]
pub struct CollaborationContextRequest {
    pub run_id: Uuid,
    pub task_id: Uuid,
    pub work_state: Option<WorkStateSummary>,
    pub readable_component_keys: Vec<String>,
    pub allowed_capabilities: Vec<ReleaseIdentity>,
    pub expected_deliverable_schemas: Vec<String>,
    pub blocking_input_policy: String,
    pub summary_budget_chars: u32,
}

/// Compiler for bounded, typed collaboration context. This is deliberately
/// independent of Codex scheduling and can be used by any domain package.
pub struct CollaborationContextBuilder;

impl CollaborationContextBuilder {
    pub fn build(
        request: CollaborationContextRequest,
    ) -> Result<CollaborationContext, WorkStateServiceError> {
        if request.summary_budget_chars == 0 || request.summary_budget_chars > 8_000 {
            return Err(WorkStateServiceError::InvalidContract(
                "summary budget must be between 1 and 8000 characters".to_string(),
            ));
        }
        validate_key(&request.blocking_input_policy, "blocking input policy")?;
        let mut readable_keys = std::collections::BTreeSet::new();
        for key in &request.readable_component_keys {
            validate_key(key, "readable component key")?;
            if !readable_keys.insert(key) {
                return Err(WorkStateServiceError::InvalidContract(
                    "readable component keys must be unique".to_string(),
                ));
            }
        }
        let readable_components = match &request.work_state {
            Some(state) => {
                let components = state
                    .components
                    .iter()
                    .filter(|component| readable_keys.contains(&component.key))
                    .cloned()
                    .collect::<Vec<_>>();
                if components.len() != readable_keys.len() {
                    return Err(WorkStateServiceError::InvalidContract(
                        "assignment references an unavailable Work State component".to_string(),
                    ));
                }
                components
            }
            None if readable_keys.is_empty() => Vec::new(),
            None => {
                return Err(WorkStateServiceError::InvalidContract(
                    "assignment cannot name readable components without a Work State".to_string(),
                ))
            }
        };
        let mut capability_ids = std::collections::BTreeSet::new();
        for capability in &request.allowed_capabilities {
            validate_key(&capability.resource_id, "capability resource id")?;
            validate_key(&capability.release_version, "capability release version")?;
            if !is_sha256(&capability.content_sha256)
                || !is_sha256(&capability.execution_semantics_sha256)
                || !capability_ids.insert(capability.id)
            {
                return Err(WorkStateServiceError::InvalidContract(
                    "assignment capabilities must be unique exact Releases".to_string(),
                ));
            }
        }
        for schema in &request.expected_deliverable_schemas {
            validate_key(schema, "expected deliverable schema")?;
        }
        Ok(CollaborationContext {
            run_id: request.run_id,
            task_id: request.task_id,
            work_state_id: request.work_state.as_ref().map(|state| state.id),
            readable_components,
            allowed_capabilities: request.allowed_capabilities,
            expected_deliverable_schemas: request.expected_deliverable_schemas,
            blocking_input_policy: request.blocking_input_policy,
            summary_budget_chars: request.summary_budget_chars,
        })
    }
}

/// Compiles a single immutable assignment contract. It intentionally returns a
/// typed DTO rather than generating a prompt; Codex remains the owner of
/// model-visible message construction and context compaction.
pub struct AssignmentCompiler;

impl AssignmentCompiler {
    pub fn compile(
        objective: String,
        completion_criteria: Vec<String>,
        context: CollaborationContext,
    ) -> Result<AssignmentContract, WorkStateServiceError> {
        if objective.trim().is_empty() || objective.len() > context.summary_budget_chars as usize {
            return Err(WorkStateServiceError::InvalidContract(
                "assignment objective exceeds the summary budget".to_string(),
            ));
        }
        if completion_criteria.is_empty()
            || completion_criteria.len() > 32
            || completion_criteria
                .iter()
                .any(|criterion| criterion.trim().is_empty() || criterion.len() > 512)
        {
            return Err(WorkStateServiceError::InvalidContract(
                "assignment completion criteria are invalid".to_string(),
            ));
        }
        Ok(AssignmentContract {
            objective,
            completion_criteria,
            context,
        })
    }
}

/// Constructs the shared bounded platform Tool result. Domain packages publish
/// their rich results as immutable resources and return those references here.
pub fn platform_tool_result(
    status: PlatformToolResultStatus,
    summary: String,
    references: Vec<WorkResourceReference>,
    blocking_input_ids: Vec<Uuid>,
) -> Result<PlatformToolResult, WorkStateServiceError> {
    if summary.trim().is_empty() || summary.len() > 2_000 {
        return Err(WorkStateServiceError::InvalidContract(
            "platform Tool result summary is invalid".to_string(),
        ));
    }
    if references.len() > 32 || blocking_input_ids.len() > 16 {
        return Err(WorkStateServiceError::InvalidContract(
            "platform Tool result exceeds its bounded reference limits".to_string(),
        ));
    }
    for reference in &references {
        reference_json(reference)?;
    }
    if status == PlatformToolResultStatus::NeedsInput && blocking_input_ids.is_empty() {
        return Err(WorkStateServiceError::InvalidContract(
            "needs_input results require blocking input identities".to_string(),
        ));
    }
    Ok(PlatformToolResult {
        schema_version: "platform-tool-result.v1".to_string(),
        status,
        summary,
        references,
        blocking_input_ids,
    })
}

#[derive(Clone)]
pub struct WorkStateService {
    db: PgPool,
}

impl WorkStateService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn create_state(
        &self,
        request: CreateWorkStateRequest,
    ) -> Result<WorkStateSummary, WorkStateServiceError> {
        validate_definition(&request.definition)?;
        validate_key(&request.idempotency_key, "idempotency key")?;
        let mut transaction = self.db.begin().await?;
        let definition_id = upsert_definition(
            &mut transaction,
            request.actor.organization_id,
            &request.definition,
        )
        .await?;
        let inserted_state_id: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO work_states (organization_id, profile_id, workspace_id, task_id, definition_id, idempotency_key) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (organization_id, task_id, idempotency_key) DO NOTHING \
             RETURNING id",
        )
        .bind(request.actor.organization_id)
        .bind(request.actor.profile_id)
        .bind(request.workspace_id)
        .bind(request.task_id)
        .bind(definition_id)
        .bind(&request.idempotency_key)
        .fetch_optional(&mut *transaction)
        .await?;

        let (state_id, is_new) = match inserted_state_id {
            Some(state_id) => (state_id, true),
            None => {
                let existing = sqlx::query(
                    "SELECT id, profile_id, workspace_id, definition_id
                     FROM work_states
                     WHERE organization_id = $1 AND task_id = $2 AND idempotency_key = $3
                     FOR UPDATE",
                )
                .bind(request.actor.organization_id)
                .bind(request.task_id)
                .bind(&request.idempotency_key)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or(WorkStateServiceError::OperationConflict)?;
                let existing_id: Uuid = existing.get("id");
                let same_request = existing.get::<Uuid, _>("profile_id")
                    == request.actor.profile_id
                    && existing.get::<Uuid, _>("workspace_id") == request.workspace_id
                    && existing.get::<Uuid, _>("definition_id") == definition_id;
                if !same_request {
                    return Err(WorkStateServiceError::InvalidContract(
                        "idempotency key is already bound to a different work state request"
                            .to_string(),
                    ));
                }
                (existing_id, false)
            }
        };

        for component in &request.definition.components {
            sqlx::query(
                "INSERT INTO work_components (work_state_id, component_key, revision, state) \
                 VALUES ($1, $2, 1, 'missing') ON CONFLICT DO NOTHING",
            )
            .bind(state_id)
            .bind(&component.key)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "INSERT INTO work_component_dependencies (work_state_id, component_key, depends_on_component_key) \
                 SELECT $1, $2, dependency.value \
                 FROM jsonb_array_elements_text($3::jsonb) AS dependency(value) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(state_id)
            .bind(&component.key)
            .bind(serde_json::to_value(&component.depends_on).map_err(|_| WorkStateServiceError::InvalidStoredData)?)
            .execute(&mut *transaction)
            .await?;
        }
        if is_new {
            sqlx::query(
                "INSERT INTO work_state_events (work_state_id, event_type, revision, summary) \
                 VALUES ($1, 'state_created', 0, 'Work state created')",
            )
            .bind(state_id)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        self.get_summary(request.actor.organization_id, state_id)
            .await
    }

    pub async fn get_summary(
        &self,
        organization_id: Uuid,
        state_id: Uuid,
    ) -> Result<WorkStateSummary, WorkStateServiceError> {
        load_summary(&self.db, organization_id, state_id).await
    }

    pub async fn begin_operation(
        &self,
        actor: &WorkStateActor,
        request: BeginWorkOperationRequest,
    ) -> Result<WorkOperationSummary, WorkStateServiceError> {
        validate_key(&request.kind, "operation kind")?;
        validate_key(&request.idempotency_key, "idempotency key")?;
        let mut transaction = self.db.begin().await?;
        assert_state_access(&mut transaction, actor.organization_id, request.state_id).await?;
        let operation = sqlx::query(
            "INSERT INTO work_operations (work_state_id, kind, status, idempotency_key) \
             VALUES ($1, $2, 'running', $3) \
             ON CONFLICT (work_state_id, idempotency_key) DO UPDATE SET idempotency_key = EXCLUDED.idempotency_key \
             WHERE work_operations.kind = EXCLUDED.kind \
             RETURNING id, kind, status, idempotency_key, started_at, terminal_at, failure_code, summary",
        )
        .bind(request.state_id)
        .bind(&request.kind)
        .bind(&request.idempotency_key)
        .fetch_optional(&mut *transaction)
        .await?;
        let summary = operation
            .as_ref()
            .map(operation_from_row)
            .transpose()?
            .ok_or(WorkStateServiceError::OperationConflict)?;
        if summary.status != WorkOperationStatus::Running {
            transaction.commit().await?;
            return Ok(summary);
        }
        for (name, resource) in request.input_references {
            validate_key(&name, "input name")?;
            sqlx::query(
                "INSERT INTO work_operation_inputs (operation_id, name, resource_ref) VALUES ($1, $2, $3) \
                 ON CONFLICT (operation_id, name) DO NOTHING",
            )
            .bind(summary.id)
            .bind(name)
            .bind(reference_json(&resource)?)
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            "INSERT INTO work_state_events (work_state_id, operation_id, event_type, revision, summary) \
             SELECT id, $2, 'operation_started', revision, $3 FROM work_states WHERE id = $1",
        )
        .bind(request.state_id)
        .bind(summary.id)
        .bind(&summary.kind)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(summary)
    }

    pub async fn apply_mutation(
        &self,
        actor: &WorkStateActor,
        request: ApplyWorkMutationRequest,
    ) -> Result<WorkStateSummary, WorkStateServiceError> {
        validate_mutation(&request.mutation)?;
        let mut transaction = self.db.begin().await?;
        let current_revision =
            lock_state_revision(&mut transaction, actor.organization_id, request.state_id).await?;
        if current_revision != request.mutation.expected_revision {
            return Err(WorkStateServiceError::RevisionConflict);
        }
        assert_running_operation(&mut transaction, request.state_id, request.operation_id).await?;
        assert_declared_component_keys(
            &mut transaction,
            request.state_id,
            &request.mutation.components,
        )
        .await?;
        let next_revision = current_revision + 1;
        let changed = request
            .mutation
            .components
            .iter()
            .map(|component| component.key.clone())
            .collect::<Vec<_>>();
        for component in &request.mutation.components {
            insert_component_revision(
                &mut transaction,
                request.state_id,
                request.operation_id,
                component,
            )
            .await?;
        }
        invalidate_dependents(
            &mut transaction,
            request.state_id,
            request.operation_id,
            &changed,
        )
        .await?;
        for blocker in &request.mutation.blocking_inputs {
            sqlx::query(
                "INSERT INTO work_blocking_inputs (id, work_state_id, source_operation_id, code, prompt) \
                 VALUES ($1, $2, $3, $4, $5) ON CONFLICT (id) DO NOTHING",
            )
            .bind(blocker.id)
            .bind(request.state_id)
            .bind(request.operation_id)
            .bind(&blocker.code)
            .bind(&blocker.prompt)
            .execute(&mut *transaction)
            .await?;
        }
        for deliverable in &request.mutation.deliverables {
            sqlx::query(
                "INSERT INTO work_deliverables (id, work_state_id, operation_id, schema, display_name, resource_ref) \
                 VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (id) DO NOTHING",
            )
            .bind(deliverable.id)
            .bind(request.state_id)
            .bind(request.operation_id)
            .bind(&deliverable.schema)
            .bind(&deliverable.display_name)
            .bind(reference_json(&deliverable.resource)?)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "INSERT INTO work_operation_outputs (operation_id, name, resource_ref) VALUES ($1, $2, $3) \
                 ON CONFLICT (operation_id, name) DO NOTHING",
            )
            .bind(request.operation_id)
            .bind(format!("deliverable:{}", deliverable.id))
            .bind(reference_json(&deliverable.resource)?)
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query("UPDATE work_states SET revision = $1, updated_at = now() WHERE id = $2")
            .bind(next_revision)
            .bind(request.state_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "UPDATE work_operations SET status = 'completed', terminal_at = now(), summary = $1, updated_at = now() \
             WHERE id = $2 AND status = 'running'",
        )
        .bind(request.mutation.summary.as_deref())
        .bind(request.operation_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO work_state_events (work_state_id, operation_id, event_type, revision, summary) \
             VALUES ($1, $2, 'mutation_applied', $3, $4)",
        )
        .bind(request.state_id)
        .bind(request.operation_id)
        .bind(next_revision)
        .bind(request.mutation.summary.as_deref())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.get_summary(actor.organization_id, request.state_id)
            .await
    }

    pub async fn fail_operation(
        &self,
        actor: &WorkStateActor,
        state_id: Uuid,
        operation_id: Uuid,
        status: WorkOperationStatus,
        failure_code: Option<&str>,
        summary: Option<&str>,
    ) -> Result<WorkOperationSummary, WorkStateServiceError> {
        if !matches!(
            status,
            WorkOperationStatus::Failed
                | WorkOperationStatus::Rejected
                | WorkOperationStatus::Cancelled
                | WorkOperationStatus::Timeout
                | WorkOperationStatus::Interrupted
        ) {
            return Err(WorkStateServiceError::OperationConflict);
        }
        let mut transaction = self.db.begin().await?;
        assert_state_access(&mut transaction, actor.organization_id, state_id).await?;
        let row = sqlx::query(
            "UPDATE work_operations SET status = $1, failure_code = $2, summary = $3, terminal_at = now(), updated_at = now() \
             WHERE id = $4 AND work_state_id = $5 AND status IN ('pending', 'running') \
             RETURNING id, kind, status, idempotency_key, started_at, terminal_at, failure_code, summary",
        )
        .bind(operation_status_name(&status))
        .bind(failure_code)
        .bind(summary)
        .bind(operation_id)
        .bind(state_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            return Err(WorkStateServiceError::OperationConflict);
        };
        let result = operation_from_row(&row)?;
        sqlx::query(
            "INSERT INTO work_state_events (work_state_id, operation_id, event_type, revision, summary) \
             SELECT id, $2, $3, revision, $4 FROM work_states WHERE id = $1",
        )
        .bind(state_id)
        .bind(operation_id)
        .bind(format!("operation_{}", operation_status_name(&status)))
        .bind(summary)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result)
    }

    pub async fn cancel_operation(
        &self,
        actor: &WorkStateActor,
        state_id: Uuid,
        operation_id: Uuid,
        summary: Option<&str>,
    ) -> Result<WorkOperationSummary, WorkStateServiceError> {
        self.fail_operation(
            actor,
            state_id,
            operation_id,
            WorkOperationStatus::Cancelled,
            Some("cancelled"),
            summary,
        )
        .await
    }

    pub async fn mark_timeout(
        &self,
        actor: &WorkStateActor,
        state_id: Uuid,
        operation_id: Uuid,
        summary: Option<&str>,
    ) -> Result<WorkOperationSummary, WorkStateServiceError> {
        self.fail_operation(
            actor,
            state_id,
            operation_id,
            WorkOperationStatus::Timeout,
            Some("timeout"),
            summary,
        )
        .await
    }

    pub async fn list_blocking_inputs(
        &self,
        organization_id: Uuid,
        state_id: Uuid,
    ) -> Result<Vec<WorkBlockingInputSummary>, WorkStateServiceError> {
        Ok(self
            .get_summary(organization_id, state_id)
            .await?
            .blocking_inputs)
    }

    pub async fn list_deliverables(
        &self,
        organization_id: Uuid,
        state_id: Uuid,
    ) -> Result<Vec<WorkDeliverableSummary>, WorkStateServiceError> {
        Ok(self
            .get_summary(organization_id, state_id)
            .await?
            .deliverables)
    }
}

async fn upsert_definition(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    definition: &WorkStateDefinition,
) -> Result<Uuid, WorkStateServiceError> {
    let row = sqlx::query(
        "INSERT INTO work_state_definitions (id, organization_id, definition_id, version, content_sha256, component_schema) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (organization_id, definition_id, version) DO UPDATE SET definition_id = EXCLUDED.definition_id \
         WHERE work_state_definitions.content_sha256 = EXCLUDED.content_sha256 \
         RETURNING id",
    )
    .bind(definition.id)
    .bind(organization_id)
    .bind(&definition.definition_id)
    .bind(&definition.version)
    .bind(&definition.content_sha256)
    .bind(serde_json::to_value(&definition.components).map_err(|_| WorkStateServiceError::InvalidStoredData)?)
    .fetch_optional(&mut **transaction)
    .await?;
    row.map(|row| row.get("id")).ok_or_else(|| {
        WorkStateServiceError::InvalidContract(
            "definition version already has different immutable content".to_string(),
        )
    })
}

async fn assert_state_access(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    state_id: Uuid,
) -> Result<(), WorkStateServiceError> {
    let found = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM work_states WHERE id = $1 AND organization_id = $2)",
    )
    .bind(state_id)
    .bind(organization_id)
    .fetch_one(&mut **transaction)
    .await?;
    if found {
        Ok(())
    } else {
        Err(WorkStateServiceError::NotFound)
    }
}

async fn lock_state_revision(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    state_id: Uuid,
) -> Result<i64, WorkStateServiceError> {
    sqlx::query_scalar(
        "SELECT revision FROM work_states WHERE id = $1 AND organization_id = $2 FOR UPDATE",
    )
    .bind(state_id)
    .bind(organization_id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(WorkStateServiceError::NotFound)
}

async fn assert_running_operation(
    transaction: &mut Transaction<'_, Postgres>,
    state_id: Uuid,
    operation_id: Uuid,
) -> Result<(), WorkStateServiceError> {
    let found = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM work_operations WHERE id = $1 AND work_state_id = $2 AND status = 'running' FOR UPDATE",
    )
    .bind(operation_id)
    .bind(state_id)
    .fetch_optional(&mut **transaction)
    .await?;
    if found.is_some() {
        Ok(())
    } else {
        Err(WorkStateServiceError::OperationConflict)
    }
}

async fn assert_declared_component_keys(
    transaction: &mut Transaction<'_, Postgres>,
    state_id: Uuid,
    components: &[WorkComponentMutation],
) -> Result<(), WorkStateServiceError> {
    let schema: Value = sqlx::query_scalar(
        "SELECT definition.component_schema FROM work_states state \
         JOIN work_state_definitions definition ON definition.id = state.definition_id \
         WHERE state.id = $1",
    )
    .bind(state_id)
    .fetch_one(&mut **transaction)
    .await?;
    let declared: Vec<open_web_codex_platform_contracts::WorkComponentDefinition> =
        serde_json::from_value(schema).map_err(|_| WorkStateServiceError::InvalidStoredData)?;
    let keys = declared
        .into_iter()
        .map(|component| component.key)
        .collect::<std::collections::BTreeSet<_>>();
    if components
        .iter()
        .any(|component| !keys.contains(&component.key))
    {
        return Err(WorkStateServiceError::InvalidContract(
            "mutation references an undeclared component".to_string(),
        ));
    }
    Ok(())
}

async fn insert_component_revision(
    transaction: &mut Transaction<'_, Postgres>,
    state_id: Uuid,
    operation_id: Uuid,
    component: &WorkComponentMutation,
) -> Result<(), WorkStateServiceError> {
    let next_revision: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision), 0) + 1 FROM work_components WHERE work_state_id = $1 AND component_key = $2",
    ).bind(state_id).bind(&component.key).fetch_one(&mut **transaction).await?;
    sqlx::query(
        "INSERT INTO work_components (work_state_id, component_key, revision, state, resource_ref, summary, operation_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    ).bind(state_id).bind(&component.key).bind(next_revision).bind(component_state_name(&component.state))
        .bind(component.resource.as_ref().map(reference_json).transpose()?).bind(component.summary.as_deref()).bind(operation_id)
        .execute(&mut **transaction).await?;
    Ok(())
}

async fn invalidate_dependents(
    transaction: &mut Transaction<'_, Postgres>,
    state_id: Uuid,
    operation_id: Uuid,
    changed: &[String],
) -> Result<(), WorkStateServiceError> {
    let dependents = sqlx::query_scalar::<_, String>(
        "WITH RECURSIVE downstream(component_key) AS ( \
             SELECT component_key FROM work_component_dependencies \
             WHERE work_state_id = $1 AND depends_on_component_key = ANY($2) \
             UNION \
             SELECT dependency.component_key FROM work_component_dependencies dependency \
             JOIN downstream ON dependency.depends_on_component_key = downstream.component_key \
             WHERE dependency.work_state_id = $1 \
         ) SELECT component_key FROM downstream",
    )
    .bind(state_id)
    .bind(changed)
    .fetch_all(&mut **transaction)
    .await?;
    for dependent in dependents {
        let current = sqlx::query(
                "SELECT state, resource_ref, summary FROM work_components WHERE work_state_id = $1 AND component_key = $2 ORDER BY revision DESC LIMIT 1",
            ).bind(state_id).bind(&dependent).fetch_optional(&mut **transaction).await?;
        if current
            .as_ref()
            .is_some_and(|row| row.get::<String, _>("state") == "invalidated")
        {
            continue;
        }
        let next_revision: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(revision), 0) + 1 FROM work_components WHERE work_state_id = $1 AND component_key = $2")
                .bind(state_id).bind(&dependent).fetch_one(&mut **transaction).await?;
        let resource: Option<Value> = current.as_ref().and_then(|row| row.get("resource_ref"));
        sqlx::query(
                "INSERT INTO work_components (work_state_id, component_key, revision, state, resource_ref, summary, operation_id) \
                 VALUES ($1, $2, $3, 'invalidated', $4, 'Invalidated by an upstream component change', $5)",
            ).bind(state_id).bind(&dependent).bind(next_revision).bind(resource).bind(operation_id).execute(&mut **transaction).await?;
    }
    Ok(())
}

async fn load_summary(
    db: &PgPool,
    organization_id: Uuid,
    state_id: Uuid,
) -> Result<WorkStateSummary, WorkStateServiceError> {
    let row = sqlx::query(
        "SELECT state.id, state.task_id, state.workspace_id, state.revision, state.state, state.updated_at, \
                definition.id AS definition_uuid, definition.definition_id, definition.version, definition.content_sha256, definition.component_schema \
         FROM work_states state JOIN work_state_definitions definition ON definition.id = state.definition_id \
         WHERE state.id = $1 AND state.organization_id = $2",
    ).bind(state_id).bind(organization_id).fetch_optional(db).await?.ok_or(WorkStateServiceError::NotFound)?;
    let components_schema: Vec<open_web_codex_platform_contracts::WorkComponentDefinition> =
        serde_json::from_value(row.get("component_schema"))
            .map_err(|_| WorkStateServiceError::InvalidStoredData)?;
    let definition = WorkStateDefinition {
        id: row.get("definition_uuid"),
        definition_id: row.get("definition_id"),
        version: row.get("version"),
        content_sha256: row.get("content_sha256"),
        components: components_schema,
    };
    let components = sqlx::query(
        "SELECT DISTINCT ON (component_key) component_key, revision, state, resource_ref, summary \
         FROM work_components WHERE work_state_id = $1 ORDER BY component_key, revision DESC",
    )
    .bind(state_id)
    .fetch_all(db)
    .await?
    .into_iter()
    .map(component_from_row)
    .collect::<Result<Vec<_>, _>>()?;
    let blocking_inputs = sqlx::query(
        "SELECT id, code, prompt, source_operation_id, created_at FROM work_blocking_inputs \
         WHERE work_state_id = $1 AND state = 'open' ORDER BY created_at, id",
    )
    .bind(state_id)
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|row| WorkBlockingInputSummary {
        id: row.get("id"),
        code: row.get("code"),
        prompt: row.get("prompt"),
        source_operation_id: row.get("source_operation_id"),
        created_at: row.get("created_at"),
    })
    .collect();
    let deliverables = sqlx::query(
        "SELECT id, schema, display_name, resource_ref, created_at FROM work_deliverables WHERE work_state_id = $1 ORDER BY created_at, id",
    ).bind(state_id).fetch_all(db).await?.into_iter().map(|row| Ok(WorkDeliverableSummary { id: row.get("id"), schema: row.get("schema"), display_name: row.get("display_name"), resource: parse_reference(row.get("resource_ref"))?, created_at: row.get("created_at") })).collect::<Result<Vec<_>, WorkStateServiceError>>()?;
    Ok(WorkStateSummary {
        id: row.get("id"),
        task_id: row.get("task_id"),
        workspace_id: row.get("workspace_id"),
        definition,
        revision: row.get("revision"),
        state: row.get("state"),
        components,
        blocking_inputs,
        deliverables,
        updated_at: row.get("updated_at"),
    })
}

fn component_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<WorkComponentSummary, WorkStateServiceError> {
    Ok(WorkComponentSummary {
        key: row.get("component_key"),
        revision: row.get("revision"),
        state: component_state_from_db(&row.get::<String, _>("state"))?,
        resource: row
            .get::<Option<Value>, _>("resource_ref")
            .map(parse_reference)
            .transpose()?,
        summary: row.get("summary"),
    })
}

fn operation_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<WorkOperationSummary, WorkStateServiceError> {
    Ok(WorkOperationSummary {
        id: row.get("id"),
        kind: row.get("kind"),
        status: operation_status_from_db(&row.get::<String, _>("status"))?,
        idempotency_key: row.get("idempotency_key"),
        started_at: row.get("started_at"),
        terminal_at: row.get("terminal_at"),
        failure_code: row.get("failure_code"),
        summary: row.get("summary"),
    })
}

fn validate_definition(definition: &WorkStateDefinition) -> Result<(), WorkStateServiceError> {
    validate_key(&definition.definition_id, "definition id")?;
    validate_key(&definition.version, "definition version")?;
    if !is_sha256(&definition.content_sha256) || definition.components.is_empty() {
        return Err(WorkStateServiceError::InvalidContract(
            "definition must include a SHA-256 and at least one component".to_string(),
        ));
    }
    let mut keys = std::collections::BTreeSet::new();
    for component in &definition.components {
        validate_key(&component.key, "component key")?;
        if !keys.insert(&component.key) {
            return Err(WorkStateServiceError::InvalidContract(
                "component keys must be unique".to_string(),
            ));
        }
    }
    for component in &definition.components {
        if component
            .depends_on
            .iter()
            .any(|dependency| dependency == &component.key || !keys.contains(dependency))
        {
            return Err(WorkStateServiceError::InvalidContract(
                "component dependencies must reference a distinct declared component".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_mutation(mutation: &WorkStateMutation) -> Result<(), WorkStateServiceError> {
    if mutation.expected_revision < 0 {
        return Err(WorkStateServiceError::InvalidContract(
            "expected revision must be non-negative".to_string(),
        ));
    }
    let mut component_keys = std::collections::BTreeSet::new();
    for component in &mutation.components {
        validate_key(&component.key, "component key")?;
        if !component_keys.insert(&component.key) {
            return Err(WorkStateServiceError::InvalidContract(
                "a mutation may update each component only once".to_string(),
            ));
        }
        if component
            .summary
            .as_ref()
            .is_some_and(|value| value.len() > 2000)
        {
            return Err(WorkStateServiceError::InvalidContract(
                "component summary exceeds its budget".to_string(),
            ));
        }
        if let Some(reference) = &component.resource {
            reference_json(reference)?;
        }
    }
    for blocker in &mutation.blocking_inputs {
        validate_key(&blocker.code, "blocking input code")?;
        if blocker.prompt.is_empty() || blocker.prompt.len() > 1000 {
            return Err(WorkStateServiceError::InvalidContract(
                "blocking input prompt is invalid".to_string(),
            ));
        }
    }
    for deliverable in &mutation.deliverables {
        validate_key(&deliverable.schema, "deliverable schema")?;
        reference_json(&deliverable.resource)?;
    }
    Ok(())
}

fn validate_key(value: &str, label: &str) -> Result<(), WorkStateServiceError> {
    if value.trim().is_empty() || value.len() > 128 {
        Err(WorkStateServiceError::InvalidContract(format!(
            "{label} is invalid"
        )))
    } else {
        Ok(())
    }
}
fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
fn reference_json(reference: &WorkResourceReference) -> Result<Value, WorkStateServiceError> {
    validate_key(&reference.owner, "resource owner")?;
    validate_key(&reference.resource_type, "resource type")?;
    validate_key(&reference.resource_id, "resource id")?;
    if !is_sha256(&reference.content_sha256) {
        return Err(WorkStateServiceError::InvalidContract(
            "resource reference requires a SHA-256".to_string(),
        ));
    }
    serde_json::to_value(reference).map_err(|_| WorkStateServiceError::InvalidStoredData)
}
fn parse_reference(value: Value) -> Result<WorkResourceReference, WorkStateServiceError> {
    serde_json::from_value(value).map_err(|_| WorkStateServiceError::InvalidStoredData)
}
fn component_state_name(state: &WorkComponentState) -> &'static str {
    match state {
        WorkComponentState::Missing => "missing",
        WorkComponentState::Ready => "ready",
        WorkComponentState::Invalidated => "invalidated",
        WorkComponentState::Failed => "failed",
    }
}
fn component_state_from_db(value: &str) -> Result<WorkComponentState, WorkStateServiceError> {
    match value {
        "missing" => Ok(WorkComponentState::Missing),
        "ready" => Ok(WorkComponentState::Ready),
        "invalidated" => Ok(WorkComponentState::Invalidated),
        "failed" => Ok(WorkComponentState::Failed),
        _ => Err(WorkStateServiceError::InvalidStoredData),
    }
}
fn operation_status_name(status: &WorkOperationStatus) -> &'static str {
    match status {
        WorkOperationStatus::Pending => "pending",
        WorkOperationStatus::Running => "running",
        WorkOperationStatus::Completed => "completed",
        WorkOperationStatus::Failed => "failed",
        WorkOperationStatus::Rejected => "rejected",
        WorkOperationStatus::Cancelled => "cancelled",
        WorkOperationStatus::Timeout => "timeout",
        WorkOperationStatus::Interrupted => "interrupted",
    }
}
fn operation_status_from_db(value: &str) -> Result<WorkOperationStatus, WorkStateServiceError> {
    match value {
        "pending" => Ok(WorkOperationStatus::Pending),
        "running" => Ok(WorkOperationStatus::Running),
        "completed" => Ok(WorkOperationStatus::Completed),
        "failed" => Ok(WorkOperationStatus::Failed),
        "rejected" => Ok(WorkOperationStatus::Rejected),
        "cancelled" => Ok(WorkOperationStatus::Cancelled),
        "timeout" => Ok(WorkOperationStatus::Timeout),
        "interrupted" => Ok(WorkOperationStatus::Interrupted),
        _ => Err(WorkStateServiceError::InvalidStoredData),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn definition_rejects_unknown_dependency() {
        let definition = WorkStateDefinition {
            id: Uuid::now_v7(),
            definition_id: "quality-analysis".to_string(),
            version: "1".to_string(),
            content_sha256: "a".repeat(64),
            components: vec![open_web_codex_platform_contracts::WorkComponentDefinition {
                key: "dataset".to_string(),
                display_name: "Dataset".to_string(),
                required: true,
                resource_types: vec!["dataset.v1".to_string()],
                depends_on: vec!["unknown".to_string()],
            }],
        };
        assert!(matches!(
            validate_definition(&definition),
            Err(WorkStateServiceError::InvalidContract(_))
        ));
    }
    #[test]
    fn resource_references_require_content_identity() {
        let reference = WorkResourceReference {
            owner: "artifact".to_string(),
            resource_type: "dataset.v1".to_string(),
            resource_id: "dataset-1".to_string(),
            content_sha256: "not-a-hash".to_string(),
        };
        assert!(matches!(
            reference_json(&reference),
            Err(WorkStateServiceError::InvalidContract(_))
        ));
    }

    #[test]
    fn platform_results_require_blocker_identity_when_input_is_needed() {
        assert!(matches!(
            platform_tool_result(
                PlatformToolResultStatus::NeedsInput,
                "Need a business parameter".to_string(),
                Vec::new(),
                Vec::new(),
            ),
            Err(WorkStateServiceError::InvalidContract(_))
        ));
    }
}
