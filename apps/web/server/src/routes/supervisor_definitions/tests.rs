use axum::{
    extract::{Path, State},
    Json,
};
use open_web_codex_platform_contracts::{
    PublishSupervisorDraftRequest, SupervisorAgentSelection, SupervisorArtifactContractInput,
    SupervisorDraftRequest, SupervisorDraftUpdateRequest, SupervisorInstructionPolicySelection,
    SupervisorPolicySelection,
};
use open_web_codex_platform_store::AppState;
use uuid::Uuid;

use super::{create, publish, release_spec_from_draft, save_draft};
use crate::middleware::auth::AuthenticatedUser;

fn valid_draft() -> SupervisorDraftRequest {
    let package =
        open_web_codex_supervisor_catalog::supervisor::resolve(&SupervisorPolicySelection {
            policy_id: "enterprise-supervisor-copilot".to_string(),
            version: "6.0.0".to_string(),
        })
        .expect("current repository Supervisor package");
    SupervisorDraftRequest {
        policy_id: "network-supervisor".to_string(),
        display_name: package.display_name,
        description: package.description,
        responsibilities: package.responsibilities,
        instruction_policy: SupervisorInstructionPolicySelection {
            policy_id: package.instruction_policy.policy_id,
            version: package.instruction_policy.version,
        },
        custom_instructions: package.custom_instructions,
        agents: package
            .agents
            .into_iter()
            .map(|agent| SupervisorAgentSelection {
                definition_id: agent.definition_id,
                version: agent.version,
                release_id: agent.release_id,
                spawn_limit: agent.spawn_limit,
            })
            .collect(),
        artifact_contracts: package
            .artifact_contracts
            .into_iter()
            .map(|contract| SupervisorArtifactContractInput {
                artifact_type: contract.artifact_type,
                producer_agent: contract.producer_agent,
                consumer_agents: contract.consumer_agents,
                required: contract.required,
            })
            .collect(),
        data_requirement_contracts: package.data_requirement_contracts,
        coordination_capabilities: package.coordination_capabilities,
        max_active_child_agents: package.max_active_child_agents,
    }
}

#[test]
fn resolves_browser_draft_through_server_owned_runtime_facts() {
    let agents = open_web_codex_supervisor_catalog::agent::list_resolved_builtins().unwrap();
    let package = open_web_codex_supervisor_catalog::supervisor::validate_release_with_agents(
        release_spec_from_draft(&valid_draft(), &agents).unwrap(),
        &agents,
    )
    .unwrap();
    assert_eq!(package.required_runtime_roles.len(), 2);
    assert_eq!(
        package
            .runtime_requirements
            .iter()
            .map(|requirement| requirement.capability_id.as_str())
            .collect::<Vec<_>>(),
        vec!["agents.multi_agent"]
    );
    assert_eq!(package.content_sha256.len(), 64);
}

#[test]
fn rejects_unpublished_agent_versions_without_fallback() {
    let mut draft = valid_draft();
    draft.agents[0].version = "1.6.0".to_string();
    let agents = open_web_codex_supervisor_catalog::agent::list_resolved_builtins().unwrap();
    let issue = release_spec_from_draft(&draft, &agents).unwrap_err();
    assert_eq!(issue.code, "agent_not_published");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing at a disposable PostgreSQL database"]
async fn publishes_and_resolves_an_organization_scoped_release() {
    let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect disposable PostgreSQL database");
    open_web_codex_platform_store::migrate::run(&pool)
        .await
        .expect("migrate database");
    let organization_id = Uuid::now_v7();
    let other_organization_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO organizations (id, name, slug) VALUES \
         ($1, 'Supervisor Studio', $2), ($3, 'Other Organization', $4)",
    )
    .bind(organization_id)
    .bind(format!("supervisor-studio-{organization_id}"))
    .bind(other_organization_id)
    .bind(format!("other-supervisor-{other_organization_id}"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO users (id, username, name, email, password_hash, role) \
         VALUES ($1, $2, 'Studio Owner', $3, 'test-only', 'owner')",
    )
    .bind(user_id)
    .bind(format!("studio-{user_id}"))
    .bind(format!("{user_id}@example.invalid"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memberships (organization_id, user_id, role) \
         VALUES ($1, $2, 'owner')",
    )
    .bind(organization_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    let auth = AuthenticatedUser {
        session_id: Uuid::now_v7(),
        user_id,
        name: "Studio Owner".to_string(),
        username: format!("studio-{user_id}"),
        email: format!("{user_id}@example.invalid"),
        role: "owner".to_string(),
        organization_id,
        organization_role: "owner".to_string(),
    };
    let state = AppState::new(pool.clone());
    let definition = create(State(state.clone()), auth.clone(), Json(valid_draft()))
        .await
        .unwrap()
        .0;
    let mut edited_draft = valid_draft();
    edited_draft.display_name = "Network Supervisor revised".to_string();
    let saved = save_draft(
        State(state.clone()),
        auth.clone(),
        Path(definition.id),
        Json(SupervisorDraftUpdateRequest {
            draft: edited_draft.clone(),
            expected_revision: 1,
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(saved.draft_metadata.unwrap().revision, 2);
    let stale = save_draft(
        State(state.clone()),
        auth.clone(),
        Path(definition.id),
        Json(SupervisorDraftUpdateRequest {
            draft: valid_draft(),
            expected_revision: 1,
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(stale.0, axum::http::StatusCode::CONFLICT);
    let validation = super::validate(State(state.clone()), auth.clone(), Path(definition.id))
        .await
        .unwrap()
        .0;
    assert!(validation.valid);
    let release = publish(
        State(state),
        auth,
        Path(definition.id),
        Json(PublishSupervisorDraftRequest {
            expected_revision: 2,
        }),
    )
    .await
    .unwrap()
    .0;
    let selection = SupervisorPolicySelection {
        policy_id: release.policy_id,
        version: release.version,
    };
    let resolved =
        crate::supervisor_policy::resolve_for_new_run(&pool, organization_id, &selection, None)
            .await
            .unwrap();
    assert_eq!(
        resolved.snapshot.source,
        open_web_codex_run_orchestrator::SupervisorPolicySource::UserRelease
    );
    assert_eq!(resolved.snapshot.release_id, Some(release.id));
    assert_eq!(resolved.snapshot.content_sha256, release.content_sha256);
    assert_eq!(
        crate::supervisor_policy::resolve_for_new_run(
            &pool,
            other_organization_id,
            &selection,
            None
        )
        .await
        .unwrap_err(),
        crate::supervisor_policy::SupervisorPolicyError::NotFound
    );
}
