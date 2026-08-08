use open_web_codex_platform_contracts::SupervisorPolicySelection;
use serde_json::{json, Value};

use super::{require_runtime_manifest, resolve_builtin, SupervisorPolicyError};

#[test]
fn resolves_the_current_repository_supervisor_release_without_fallback() {
    let published = open_web_codex_supervisor_catalog::supervisor::list_published().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].version, "6.0.0");
    assert_eq!(
        resolve_builtin(&SupervisorPolicySelection {
            policy_id: "enterprise-supervisor-copilot".to_string(),
            version: "5.0.1".to_string(),
        })
        .unwrap_err(),
        SupervisorPolicyError::NotFound
    );
}

fn generated_manifest() -> Value {
    serde_json::from_str(include_str!(
        "../../contracts/codex/fixtures/capability-manifest.v1.json"
    ))
    .expect("generated capability manifest fixture")
}

fn capability_mut<'a>(manifest: &'a mut Value, id: &str) -> &'a mut Value {
    manifest["capabilities"]
        .as_array_mut()
        .expect("capabilities array")
        .iter_mut()
        .find(|capability| capability["id"] == id)
        .expect("declared capability")
}

#[test]
fn requires_generated_multi_agent_capability_with_exact_role_limits() {
    let manifest = generated_manifest();
    require_runtime_manifest(
        &manifest,
        &open_web_codex_supervisor_catalog::supervisor::governed_runtime_requirements(),
    )
    .unwrap();
}

#[test]
fn rejects_unavailable_multi_agent_capability() {
    let mut manifest = generated_manifest();
    capability_mut(&mut manifest, "agents.multi_agent")["status"] = json!("unsupported");
    assert_eq!(
        require_runtime_manifest(
            &manifest,
            &open_web_codex_supervisor_catalog::supervisor::governed_runtime_requirements(),
        )
        .unwrap_err(),
        SupervisorPolicyError::Capability(
            "Codex 'agents.multi_agent' capability is unavailable".to_string()
        )
    );
}

#[test]
fn rejects_multi_agent_capability_without_exact_role_allowlist() {
    let mut manifest = generated_manifest();
    capability_mut(&mut manifest, "agents.multi_agent")["limits"]
        .as_object_mut()
        .unwrap()
        .remove("exactRoleAllowlist");
    assert_eq!(
        require_runtime_manifest(
            &manifest,
            &open_web_codex_supervisor_catalog::supervisor::governed_runtime_requirements(),
        )
        .unwrap_err(),
        SupervisorPolicyError::Capability(
            "Codex 'agents.multi_agent' capability does not satisfy required limit 'exactRoleAllowlist'".to_string()
        )
    );
}

#[test]
fn rejects_multi_agent_capability_without_exact_role_instance_limits() {
    let mut manifest = generated_manifest();
    capability_mut(&mut manifest, "agents.multi_agent")["limits"]
        .as_object_mut()
        .unwrap()
        .remove("exactRoleInstanceLimits");
    assert_eq!(
        require_runtime_manifest(
            &manifest,
            &open_web_codex_supervisor_catalog::supervisor::governed_runtime_requirements(),
        )
        .unwrap_err(),
        SupervisorPolicyError::Capability(
            "Codex 'agents.multi_agent' capability does not satisfy required limit 'exactRoleInstanceLimits'".to_string()
        )
    );
}

#[test]
fn rejects_old_or_unknown_versions_without_fallback() {
    for version in ["2.0.0", "1.0.0", "9.9.9"] {
        let selection = SupervisorPolicySelection {
            policy_id: "enterprise-supervisor-copilot".to_string(),
            version: version.to_string(),
        };
        assert_eq!(
            resolve_builtin(&selection).unwrap_err(),
            SupervisorPolicyError::NotFound
        );
    }
}
