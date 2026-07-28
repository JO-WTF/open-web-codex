use axum::{
    extract::{Path, State},
    Json,
};
use open_web_codex_platform_contracts::{
    SupervisorAgentSelection, SupervisorArtifactContractInput, SupervisorDraftRequest,
    SupervisorInstructionPolicyPublishRequest, SupervisorInstructionPolicySelection,
    SupervisorPolicySelection,
};
use open_web_codex_platform_store::AppState;
use uuid::Uuid;

use super::{create, publish, release_spec_from_draft};
use crate::middleware::auth::AuthenticatedUser;

fn valid_draft() -> SupervisorDraftRequest {
    SupervisorDraftRequest {
        policy_id: "network-supervisor".to_string(),
        version: "1.0.0".to_string(),
        display_name: "Network Supervisor".to_string(),
        description: "Coordinates a bounded network-planning workflow.".to_string(),
        responsibilities: vec![
            "Coordinate typed Agent handoffs.".to_string(),
            "Publish the final recommendation.".to_string(),
        ],
        instruction_policy: SupervisorInstructionPolicySelection {
            policy_id: "platform-supervisor-behavior".to_string(),
            version: "1.0.0".to_string(),
        },
        custom_instructions: "Delegate data preparation before network scenario analysis."
            .to_string(),
        agents: vec![
            SupervisorAgentSelection {
                definition_id: "enterprise-data-agent".to_string(),
                version: "1.6.0".to_string(),
                release_id: None,
                spawn_limit: 1,
            },
            SupervisorAgentSelection {
                definition_id: "enterprise-network-planning-agent".to_string(),
                version: "1.5.0".to_string(),
                release_id: None,
                spawn_limit: 1,
            },
        ],
        artifact_contracts: vec![
            SupervisorArtifactContractInput {
                artifact_type: "planning-dataset.v1".to_string(),
                producer_agent: "enterprise-data-agent@1.6.0".to_string(),
                consumer_agents: vec!["enterprise-network-planning-agent@1.5.0".to_string()],
                required: true,
            },
            SupervisorArtifactContractInput {
                artifact_type: "scenario_comparison.v1".to_string(),
                producer_agent: "enterprise-network-planning-agent@1.5.0".to_string(),
                consumer_agents: vec!["supervisor".to_string()],
                required: true,
            },
        ],
        max_active_child_agents: 2,
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
    draft.agents[0].version = "1.5.0".to_string();
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
    let platform_policy = crate::routes::supervisor_instruction_policies::publish(
        State(state.clone()),
        auth.clone(),
        Json(SupervisorInstructionPolicyPublishRequest {
            policy_id: "platform-supervisor-behavior".to_string(),
            version: "1.1.0".to_string(),
            display_name: "Platform Supervisor behavior".to_string(),
            description: "Updated platform behavior contract.".to_string(),
            platform_instructions:
                "Delegate only to authorized Runtime Roles and preserve typed Artifacts."
                    .to_string(),
        }),
    )
    .await
    .unwrap()
    .0;
    let mut draft = valid_draft();
    draft.instruction_policy = SupervisorInstructionPolicySelection {
        policy_id: platform_policy.policy_id,
        version: platform_policy.version,
    };
    let definition = create(State(state.clone()), auth.clone(), Json(draft))
        .await
        .unwrap()
        .0;
    let validation = super::validate(State(state.clone()), auth.clone(), Path(definition.id))
        .await
        .unwrap()
        .0;
    assert!(validation.valid);
    let release = publish(State(state), auth, Path(definition.id))
        .await
        .unwrap()
        .0;
    let selection = SupervisorPolicySelection {
        policy_id: release.policy_id,
        version: release.version,
    };
    let resolved =
        crate::supervisor_policy::resolve_for_new_run(&pool, organization_id, &selection)
            .await
            .unwrap();
    assert_eq!(
        resolved.snapshot.source,
        open_web_codex_run_orchestrator::SupervisorPolicySource::UserRelease
    );
    assert_eq!(resolved.snapshot.release_id, Some(release.id));
    assert_eq!(resolved.snapshot.content_sha256, release.content_sha256);
    assert_eq!(
        crate::supervisor_policy::resolve_for_new_run(&pool, other_organization_id, &selection)
            .await
            .unwrap_err(),
        crate::supervisor_policy::SupervisorPolicyError::NotFound
    );
}
