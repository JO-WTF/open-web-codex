use open_web_codex_platform_contracts::SupervisorPolicySelection;
use serde_json::{json, Value};

use super::{require_runtime_manifest, resolve_builtin, SupervisorPolicyError};

fn published_policy() -> super::ResolvedSupervisorPolicy {
    resolve_builtin(&SupervisorPolicySelection {
        policy_id: "enterprise-supervisor-copilot".to_string(),
        version: "1.7.0".to_string(),
    })
    .unwrap()
}

#[test]
fn resolves_only_the_current_published_version_and_seals_its_content() {
    let published = open_web_codex_supervisor_catalog::supervisor::list_published().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].version, "1.7.0");

    let selected = SupervisorPolicySelection {
        policy_id: published[0].policy_id.clone(),
        version: published[0].version.clone(),
    };
    let policy = resolve_builtin(&selected).unwrap();
    assert_eq!(policy.snapshot.content_sha256.len(), 64);
    assert_eq!(policy.max_active_child_agents, 2);
    assert_eq!(
        policy.role_spawn_limits,
        [
            ("data_agent".to_string(), 1),
            ("network_planning_agent".to_string(), 1)
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(
        policy
            .required_runtime_roles
            .iter()
            .map(|role| (role.name.as_str(), role.version.as_str()))
            .collect::<Vec<_>>(),
        vec![("data_agent", "1.6.0"), ("network_planning_agent", "1.5.0")]
    );
    assert_eq!(
        policy
            .required_mcp_servers
            .iter()
            .map(|server| server.name.as_str())
            .collect::<Vec<_>>(),
        vec!["supply_chain_data", "supply_chain_planner"]
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
    let policy = published_policy();
    require_runtime_manifest(&manifest, &policy.runtime_requirements).unwrap();
}

#[test]
fn rejects_unavailable_multi_agent_capability() {
    let mut manifest = generated_manifest();
    capability_mut(&mut manifest, "agents.multi_agent")["status"] = json!("unsupported");
    let policy = published_policy();
    assert_eq!(
        require_runtime_manifest(&manifest, &policy.runtime_requirements).unwrap_err(),
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
    let policy = published_policy();
    assert_eq!(
        require_runtime_manifest(&manifest, &policy.runtime_requirements).unwrap_err(),
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
    let policy = published_policy();
    assert_eq!(
        require_runtime_manifest(&manifest, &policy.runtime_requirements).unwrap_err(),
        SupervisorPolicyError::Capability(
            "Codex 'agents.multi_agent' capability does not satisfy required limit 'exactRoleInstanceLimits'".to_string()
        )
    );
}

#[test]
fn rejects_old_or_unknown_versions_without_fallback() {
    for version in ["1.6.0", "1.0.0", "9.9.9"] {
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
