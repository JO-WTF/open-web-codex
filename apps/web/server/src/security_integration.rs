use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use open_web_codex_adapter::fake::FakeCodexAdapter;
use open_web_codex_approval_service::ApprovalService;
use open_web_codex_auth::hash_password;
use open_web_codex_git_runtime::{GitRuntime, GitRuntimeConfig};
use open_web_codex_platform_store::AppState;
use open_web_codex_provider_service::secured::InMemoryAuthorizedProviderService;
use open_web_codex_run_orchestrator::RunOrchestrator;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use tower::ServiceExt;
use uuid::Uuid;

use crate::routes::{self, RuntimeProfileBinding};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing at a disposable PostgreSQL database"]
async fn organization_and_profile_authorization_prevent_cross_tenant_access() {
    let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect disposable PostgreSQL database");
    open_web_codex_platform_store::migrate::run(&pool)
        .await
        .expect("migrate database");

    let profile = RuntimeProfileBinding {
        runtime_key: "security-test-profile".to_string(),
        name: "Security Test Profile".to_string(),
        capabilities: routes::RuntimeCapabilityState::default(),
    };
    let state = AppState::new(pool.clone());
    let approval_service = Arc::new(ApprovalService::new(pool.clone(), "security-test-profile"));
    let runner_root = tempfile::tempdir().expect("runner root");
    let git = Arc::new(
        GitRuntime::new(GitRuntimeConfig::new(runner_root.path()).with_local_sources())
            .expect("Git runtime"),
    );
    let adapter = Arc::new(FakeCodexAdapter::new().with_demo_workspace().await);
    let orchestrator = Arc::new(
        RunOrchestrator::new(
            pool.clone(),
            git.clone(),
            adapter.clone(),
            "security-test-profile",
            "security-test-worker",
            std::time::Duration::from_secs(30),
        )
        .expect("Run orchestrator"),
    );
    let app = Router::new()
        .nest(
            "/api",
            routes::router(
                adapter,
                Arc::new(InMemoryAuthorizedProviderService::default()),
                approval_service.clone(),
                git,
                orchestrator,
                profile,
                true,
            ),
        )
        .with_state(state);

    let bootstrap = call(
        &app,
        Request::post("/api/bootstrap")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "name": "First Owner",
                    "email": "first@example.invalid",
                    "password": "first-password"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(bootstrap.0, StatusCode::OK);
    let first_token = bootstrap.1["session_token"]
        .as_str()
        .expect("bootstrap session token")
        .to_string();
    let first_user_id = Uuid::parse_str(bootstrap.1["user"]["id"].as_str().unwrap()).unwrap();
    let first_organization_id =
        Uuid::parse_str(bootstrap.1["organization"]["id"].as_str().unwrap()).unwrap();

    let first_project = call(
        &app,
        authenticated_json(
            "POST",
            "/api/projects",
            &first_token,
            json!({"name": "First Project", "git_url": "https://example.invalid/first.git"}),
        ),
    )
    .await;
    assert_eq!(first_project.0, StatusCode::OK);
    let first_project_id = first_project.1["id"].as_str().unwrap().to_string();

    let first_task_id = Uuid::now_v7();
    let first_run_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO tasks (id, organization_id, project_id, title) \
         VALUES ($1, $2, $3, 'Approval Task')",
    )
    .bind(first_task_id)
    .bind(first_organization_id)
    .bind(Uuid::parse_str(&first_project_id).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO runs (id, organization_id, task_id, status, codex_thread_id) \
         VALUES ($1, $2, $3, 'running', 'approval-thread')",
    )
    .bind(first_run_id)
    .bind(first_organization_id)
    .bind(first_task_id)
    .execute(&pool)
    .await
    .unwrap();
    let approval_id = approval_service
        .capture_message(&json!({
            "id": 77,
            "method": "item/commandExecution/requestApproval",
            "params": {
                "threadId": "approval-thread",
                "turnId": "turn-1",
                "itemId": "item-1",
                "command": "git status",
                "cwd": "/private/server/path",
                "reason": "inspect changes"
            }
        }))
        .await
        .unwrap()
        .expect("captured approval");

    let pending = call(&app, authenticated("GET", "/api/approvals", &first_token)).await;
    assert_eq!(pending.0, StatusCode::OK);
    assert_eq!(pending.1[0]["id"], approval_id.to_string());
    assert_eq!(pending.1[0]["command"], "git status");
    assert!(!pending.1.to_string().contains("/private/server/path"));
    assert!(!pending.1.to_string().contains("\"77\""));

    let decided = call(
        &app,
        authenticated_json(
            "POST",
            &format!("/api/approvals/{approval_id}/decision"),
            &first_token,
            json!({"decision": "accept", "version": 0}),
        ),
    )
    .await;
    assert_eq!(decided.0, StatusCode::NO_CONTENT);
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE target_id = $1 AND action = 'approval.decide'",
    )
    .bind(approval_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);

    let second_user_id = Uuid::now_v7();
    let second_organization_id = Uuid::now_v7();
    let second_token = "second-session-token";
    sqlx::query(
        "INSERT INTO users (id, name, email, password_hash, role) \
         VALUES ($1, 'Second Owner', 'second@example.invalid', $2, 'owner')",
    )
    .bind(second_user_id)
    .bind(hash_password("second-password").unwrap())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Second', $2)")
        .bind(second_organization_id)
        .bind(format!("second-{second_organization_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO memberships (organization_id, user_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(second_organization_id)
    .bind(second_user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memberships (organization_id, user_id, role) VALUES ($1, $2, 'member')",
    )
    .bind(second_organization_id)
    .bind(first_user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sessions (user_id, organization_id, token_hash, expires_at) \
         VALUES ($1, $2, $3, now() + interval '1 hour')",
    )
    .bind(second_user_id)
    .bind(second_organization_id)
    .bind(hex::encode(Sha256::digest(second_token.as_bytes())))
    .execute(&pool)
    .await
    .unwrap();

    let second_project = call(
        &app,
        authenticated_json(
            "POST",
            "/api/projects",
            second_token,
            json!({"name": "Second Project", "git_url": "https://example.invalid/second.git"}),
        ),
    )
    .await;
    assert_eq!(second_project.0, StatusCode::OK);

    let second_list = call(&app, authenticated("GET", "/api/projects", second_token)).await;
    assert_eq!(second_list.0, StatusCode::OK);
    assert_eq!(second_list.1.as_array().unwrap().len(), 1);
    assert_eq!(second_list.1[0]["name"], "Second Project");

    let cross_tenant = call(
        &app,
        authenticated(
            "GET",
            &format!("/api/projects/{first_project_id}"),
            second_token,
        ),
    )
    .await;
    assert_eq!(cross_tenant.0, StatusCode::NOT_FOUND);

    let legacy_runtime = call(
        &app,
        authenticated_json(
            "POST",
            "/api/rpc",
            second_token,
            json!({"method": "list_workspaces", "params": {}}),
        ),
    )
    .await;
    assert_eq!(legacy_runtime.0, StatusCode::NOT_FOUND);

    let switched = call(
        &app,
        authenticated_json(
            "PUT",
            "/api/sessions/organization",
            &first_token,
            json!({"organization_id": second_organization_id}),
        ),
    )
    .await;
    assert_eq!(switched.0, StatusCode::OK);
    assert_eq!(switched.1["role"], "member");

    let member_create = call(
        &app,
        authenticated_json(
            "POST",
            "/api/projects",
            &first_token,
            json!({"name": "Denied", "git_url": "https://example.invalid/denied.git"}),
        ),
    )
    .await;
    assert_eq!(member_create.0, StatusCode::FORBIDDEN);

    let password_hash: String = sqlx::query(
        "SELECT password_hash FROM users WHERE id = $1 AND id IN \
         (SELECT user_id FROM memberships WHERE organization_id = $2)",
    )
    .bind(first_user_id)
    .bind(first_organization_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("password_hash");
    assert!(password_hash.starts_with("$argon2id$"));
}

async fn call(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.expect("HTTP response");
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body");
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).expect("JSON response")
    };
    (status, value)
}

fn authenticated(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn authenticated_json(method: &str, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
