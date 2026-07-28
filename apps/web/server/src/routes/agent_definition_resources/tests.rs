use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::{
    AgentCapabilityTemplateSelection, AgentDefinitionDraftRequest, SupervisorAgentSelection,
    SupervisorArtifactContractInput, SupervisorDraftRequest, SupervisorInstructionPolicySelection,
    SupervisorPolicySelection,
};
use open_web_codex_platform_store::AppState;
use sqlx::Row;
use uuid::Uuid;

use super::{create, publish, save_draft, validate, validate_draft};
use crate::middleware::auth::AuthenticatedUser;

fn valid_draft() -> AgentDefinitionDraftRequest {
    AgentDefinitionDraftRequest {
        definition_id: "regional-data-reviewer".to_string(),
        version: "1.0.0".to_string(),
        display_name: "Regional Data Reviewer".to_string(),
        description: "Prepares a reviewed regional planning dataset.".to_string(),
        responsibilities: vec![
            "Inspect authorized regional planning inputs.".to_string(),
            "Publish a validated planning dataset.".to_string(),
        ],
        developer_instructions:
            "Use only the authorized data capability and publish planning-dataset.v1.".to_string(),
        input_artifact_types: Vec::new(),
        output_artifact_types: vec!["planning-dataset.v1".to_string()],
        capability_template: AgentCapabilityTemplateSelection {
            definition_id: "enterprise-data-agent".to_string(),
            version: "1.6.0".to_string(),
        },
    }
}

#[test]
fn validates_a_user_agent_without_accepting_runtime_facts() {
    let result = validate_draft(&valid_draft());
    assert!(result.valid);
    assert_eq!(result.content_sha256.unwrap().len(), 64);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing at a disposable PostgreSQL database"]
async fn publishes_agent_and_uses_it_in_an_organization_supervisor() {
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
         ($1, 'Agent Studio', $2), ($3, 'Other Organization', $4)",
    )
    .bind(organization_id)
    .bind(format!("agent-studio-{organization_id}"))
    .bind(other_organization_id)
    .bind(format!("other-agent-{other_organization_id}"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO users (id, username, name, email, password_hash, role) \
         VALUES ($1, $2, 'Agent Owner', $3, 'test-only', 'owner')",
    )
    .bind(user_id)
    .bind(format!("agent-{user_id}"))
    .bind(format!("{user_id}@example.invalid"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memberships (organization_id, user_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(organization_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    let auth = AuthenticatedUser {
        session_id: Uuid::now_v7(),
        user_id,
        name: "Agent Owner".to_string(),
        username: format!("agent-{user_id}"),
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
    let mut other_auth = auth.clone();
    other_auth.organization_id = other_organization_id;
    assert_eq!(
        validate(State(state.clone()), other_auth, Path(definition.id))
            .await
            .unwrap_err()
            .0,
        StatusCode::NOT_FOUND
    );
    assert!(
        validate(State(state.clone()), auth.clone(), Path(definition.id))
            .await
            .unwrap()
            .0
            .valid
    );
    let release = publish(State(state.clone()), auth.clone(), Path(definition.id))
        .await
        .unwrap()
        .0;
    assert_eq!(
        save_draft(
            State(state.clone()),
            auth.clone(),
            Path(definition.id),
            Json(valid_draft()),
        )
        .await
        .unwrap_err()
        .0,
        StatusCode::CONFLICT
    );
    let catalog = crate::agent_catalog::list_published(&pool, organization_id)
        .await
        .unwrap();
    assert!(catalog
        .iter()
        .any(|agent| agent.release_id == Some(release.id)));
    assert!(
        !crate::agent_catalog::list_published(&pool, other_organization_id)
            .await
            .unwrap()
            .iter()
            .any(|agent| agent.release_id == Some(release.id))
    );

    let supervisor = crate::routes::supervisor_definitions::create(
        State(state.clone()),
        auth.clone(),
        Json(SupervisorDraftRequest {
            policy_id: "regional-review-supervisor".to_string(),
            version: "1.0.0".to_string(),
            display_name: "Regional Review Supervisor".to_string(),
            description: "Coordinates one governed regional data review.".to_string(),
            responsibilities: vec!["Deliver the reviewed planning dataset.".to_string()],
            instruction_policy: SupervisorInstructionPolicySelection {
                policy_id: "platform-supervisor-behavior".to_string(),
                version: "1.0.0".to_string(),
            },
            custom_instructions: "Delegate the regional data review and report the durable result."
                .to_string(),
            agents: vec![SupervisorAgentSelection {
                definition_id: release.definition_id.clone(),
                version: release.version.clone(),
                release_id: Some(release.id),
                spawn_limit: 1,
            }],
            artifact_contracts: vec![SupervisorArtifactContractInput {
                artifact_type: "planning-dataset.v1".to_string(),
                producer_agent: format!("{}@{}", release.definition_id, release.version),
                consumer_agents: vec!["supervisor".to_string()],
                required: true,
            }],
            max_active_child_agents: 1,
        }),
    )
    .await
    .unwrap()
    .0;
    let supervisor_release =
        crate::routes::supervisor_definitions::publish(State(state), auth, Path(supervisor.id))
            .await
            .unwrap()
            .0;
    let resolved = crate::supervisor_policy::resolve_for_new_run(
        &pool,
        organization_id,
        &SupervisorPolicySelection {
            policy_id: supervisor_release.policy_id,
            version: supervisor_release.version,
        },
    )
    .await
    .unwrap();
    assert_eq!(resolved.required_runtime_roles.len(), 1);
    assert!(resolved.required_runtime_roles[0]
        .name
        .starts_with("agent_"));
    let dependency = sqlx::query(
        "SELECT agent_release_id, agent_content_sha256 \
         FROM supervisor_release_agent_dependencies WHERE supervisor_release_id = $1",
    )
    .bind(supervisor_release.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(dependency.get::<Uuid, _>("agent_release_id"), release.id);
    assert_eq!(
        dependency.get::<String, _>("agent_content_sha256"),
        release.content_sha256
    );
    assert_eq!(
        crate::supervisor_policy::resolve_for_new_run(
            &pool,
            other_organization_id,
            &SupervisorPolicySelection {
                policy_id: "regional-review-supervisor".to_string(),
                version: "1.0.0".to_string(),
            },
        )
        .await
        .unwrap_err(),
        crate::supervisor_policy::SupervisorPolicyError::NotFound
    );
    sqlx::query(
        "UPDATE supervisor_release_agent_dependencies \
         SET agent_content_sha256 = $1 WHERE supervisor_release_id = $2",
    )
    .bind("0".repeat(64))
    .bind(supervisor_release.id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        crate::supervisor_policy::resolve_for_new_run(
            &pool,
            organization_id,
            &SupervisorPolicySelection {
                policy_id: "regional-review-supervisor".to_string(),
                version: "1.0.0".to_string(),
            },
        )
        .await
        .unwrap_err(),
        crate::supervisor_policy::SupervisorPolicyError::Invalid(
            "Agent Release dependencies do not match"
        )
    );
}
