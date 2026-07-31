use open_web_codex_adapter::validate_required_mcp_servers;
use open_web_codex_git_runtime::GitRuntime;
use open_web_codex_platform_contracts::{
    ProviderCatalog, ReadinessScope, RunReadiness, RunReadinessAction, RunReadinessCheck,
    RunReadinessCheckCode, RunReadinessRequest, RunReadinessStatus, RunStartPurpose,
};
use open_web_codex_supervisor_catalog::agent::ResolvedAgentDefinition;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::routes::RuntimeProfileBinding;
use crate::{agent_catalog, supervisor_policy};

const MAP_PRESENTATION_ARTIFACT_TYPE: &str = "map.v3";

pub(crate) struct EvaluatedRunReadiness {
    pub readiness: RunReadiness,
    pub agent: Option<ResolvedAgentDefinition>,
    pub supervisor_policy: Option<supervisor_policy::ResolvedSupervisorPolicy>,
}

pub(crate) async fn evaluate(
    db: &sqlx::PgPool,
    git: &GitRuntime,
    profile: &RuntimeProfileBinding,
    organization_id: Uuid,
    workspace_id: Uuid,
    request: &RunReadinessRequest,
    providers: &ProviderCatalog,
    runtime_healthy: bool,
    browser_map_configured: bool,
) -> EvaluatedRunReadiness {
    let mut checks = Vec::with_capacity(7);
    checks.push(check(
        RunReadinessCheckCode::RuntimeProfile,
        if runtime_healthy {
            RunReadinessStatus::Ready
        } else {
            RunReadinessStatus::Blocked
        },
        if runtime_healthy {
            "Codex Runtime is available."
        } else {
            "Codex Runtime is unavailable."
        },
        (!runtime_healthy).then_some(RunReadinessAction::Retry),
    ));

    let provider_ready = provider_is_available(providers, request);
    checks.push(check(
        RunReadinessCheckCode::ProviderModel,
        if provider_ready {
            RunReadinessStatus::Ready
        } else {
            RunReadinessStatus::Blocked
        },
        if provider_ready {
            "The selected Provider and model are available."
        } else {
            "Select an available Provider and model before starting."
        },
        (!provider_ready).then_some(RunReadinessAction::OpenProviderSettings),
    ));

    let mut resolved_agent = None;
    let mut resolved_policy = None;
    let mut definition_ready = request.agent.is_none() || request.supervisor_policy.is_none();
    if request.agent.is_some() && request.supervisor_policy.is_some() {
        definition_ready = false;
    } else if let Some(selection) = request.agent.as_ref() {
        match agent_catalog::resolve_new_run_selection(db, organization_id, selection).await {
            Ok(agent) => resolved_agent = Some(agent),
            Err(_) => definition_ready = false,
        }
    } else if let Some(selection) = request.supervisor_policy.as_ref() {
        match supervisor_policy::resolve_for_new_run(db, organization_id, selection).await {
            Ok(policy) => resolved_policy = Some(policy),
            Err(_) => definition_ready = false,
        }
    }
    checks.push(check(
        RunReadinessCheckCode::ExecutionDefinition,
        if definition_ready {
            RunReadinessStatus::Ready
        } else {
            RunReadinessStatus::Blocked
        },
        if definition_ready {
            "The exact execution definition is available."
        } else {
            "The selected Agent or Supervisor Release is unavailable or invalid."
        },
        (!definition_ready).then_some(RunReadinessAction::OpenAgentStudio),
    ));

    let dependencies_ready = if request.purpose == RunStartPurpose::Conversation {
        // Thread creation does not own or require Dataset Releases. It only
        // verifies the platform/runtime side of the selected execution.
        true
    } else if let Some(agent) = resolved_agent.as_ref() {
        verify_agent_dependencies(git, workspace_id, agent)
            .await
            .is_ok()
    } else if let Some(policy) = resolved_policy.as_ref() {
        verify_supervisor_dependencies(git, workspace_id, policy)
            .await
            .is_ok()
    } else {
        definition_ready
    };
    checks.push(check(
        RunReadinessCheckCode::WorkspaceDependencies,
        if dependencies_ready {
            RunReadinessStatus::Ready
        } else {
            RunReadinessStatus::Blocked
        },
        if request.purpose == RunStartPurpose::Conversation {
            "The selected Workspace is authorized; business data is collected after the Thread starts."
        } else if dependencies_ready {
            "Workspace Dataset and capability-package dependencies match their immutable Releases."
        } else {
            "Required Workspace data or capability-package content is missing or changed."
        },
        (!dependencies_ready).then_some(RunReadinessAction::OpenWorkspaceData),
    ));

    let input_binding_fingerprint = if request.purpose == RunStartPurpose::Analysis {
        if let Some(task_id) = request.task_id {
            analysis_binding_fingerprint(db, organization_id, task_id, workspace_id).await
        } else {
            None
        }
    } else {
        None
    };
    let input_ready = if request.purpose == RunStartPurpose::Conversation {
        true
    } else {
        // Analysis is never admitted without an existing Task-scoped binding.
        // A launcher that does not yet have a Task therefore remains blocked;
        // the user must enter the Thread first and complete data intake there.
        input_binding_fingerprint.is_some()
    };
    checks.push(check(
        RunReadinessCheckCode::DataIntake,
        if input_ready {
            RunReadinessStatus::Ready
        } else {
            RunReadinessStatus::Blocked
        },
        if request.purpose == RunStartPurpose::Conversation {
            "Thread can start; business data is collected inside the Thread."
        } else if input_ready {
            "Data Requirement Contract, normalized Dataset Release and Task binding are ready."
        } else {
            "Complete data intake and create an immutable Task Dataset Binding before analysis."
        },
        (!input_ready).then_some(RunReadinessAction::OpenWorkspaceData),
    ));

    let runtime_capabilities_ready = match resolved_policy.as_ref() {
        Some(policy) => profile
            .capabilities
            .get()
            .await
            .is_some_and(|capabilities| {
                supervisor_policy::require_runtime_manifest(
                    &capabilities.manifest,
                    &policy.runtime_requirements,
                )
                .is_ok()
            }),
        None => runtime_healthy,
    };
    checks.push(check(
        RunReadinessCheckCode::RuntimeCapabilities,
        if runtime_capabilities_ready {
            RunReadinessStatus::Ready
        } else {
            RunReadinessStatus::Blocked
        },
        if runtime_capabilities_ready {
            "Codex Runtime declares the required typed capabilities."
        } else {
            "Codex Runtime does not declare the capabilities required by this execution."
        },
        (!runtime_capabilities_ready).then_some(RunReadinessAction::Retry),
    ));

    let required_mcp_servers = resolved_agent
        .as_ref()
        .map(|agent| agent.required_mcp_servers.as_slice())
        .or_else(|| {
            resolved_policy
                .as_ref()
                .map(|policy| policy.required_mcp_servers.as_slice())
        })
        .unwrap_or_default();
    let mcp_ready =
        definition_ready && dependencies_ready && mcp_declarations_are_valid(required_mcp_servers);
    checks.push(check(
        RunReadinessCheckCode::McpServers,
        if mcp_ready {
            RunReadinessStatus::Ready
        } else {
            RunReadinessStatus::Blocked
        },
        if mcp_ready {
            "The exact MCP Server and Tool inventory is declared; Runtime revalidates it on the created Thread."
        } else {
            "A required MCP Server or Tool inventory is unavailable or invalid."
        },
        (!mcp_ready).then_some(RunReadinessAction::OpenMcpStatus),
    ));

    let requires_map = resolved_agent.as_ref().is_some_and(|agent| {
        artifact_types_require_map(agent.output_artifact_types.iter().map(String::as_str))
    }) || resolved_policy.as_ref().is_some_and(|policy| {
        artifact_types_require_map(
            policy
                .detail
                .artifact_contracts
                .iter()
                .map(|contract| contract.artifact_type.as_str()),
        )
    });
    let map_status = if requires_map && !browser_map_configured {
        RunReadinessStatus::Degraded
    } else {
        RunReadinessStatus::Ready
    };
    checks.push(check(
        RunReadinessCheckCode::MapPresentation,
        map_status,
        if map_status == RunReadinessStatus::Degraded {
            "Analysis and map Artifact creation can run, but a Mapbox public token is needed for the browser basemap."
        } else if requires_map {
            "Browser map presentation is configured."
        } else {
            "This execution does not require browser map presentation."
        },
        (map_status == RunReadinessStatus::Degraded)
            .then_some(RunReadinessAction::OpenMapsSettings),
    ));

    let thread_status = aggregate_status(
        &checks
            .iter()
            .filter(|check| check.code != RunReadinessCheckCode::DataIntake)
            .cloned()
            .collect::<Vec<_>>(),
    );
    let input_status = if request.purpose == RunStartPurpose::Conversation {
        RunReadinessStatus::Blocked
    } else if request.task_id.is_some() {
        if input_ready {
            RunReadinessStatus::Ready
        } else {
            RunReadinessStatus::Blocked
        }
    } else {
        RunReadinessStatus::Blocked
    };
    let analysis_status =
        if request.purpose == RunStartPurpose::Analysis && request.task_id.is_some() {
            aggregate_status(&checks)
        } else {
            RunReadinessStatus::Blocked
        };
    let status = if request.purpose == RunStartPurpose::Conversation {
        thread_status
    } else {
        aggregate_status(&checks)
    };
    let resolved_content_sha256 = resolved_agent
        .as_ref()
        .map(|agent| agent.content_sha256.as_str())
        .or_else(|| {
            resolved_policy
                .as_ref()
                .map(|policy| policy.snapshot.content_sha256.as_str())
        });
    let runtime_manifest_sha256 = profile.capabilities.get().await.and_then(|record| {
        serde_json::to_vec(&record.manifest)
            .ok()
            .map(|bytes| hex::encode(Sha256::digest(bytes)))
    });
    let fingerprint = evaluation_fingerprint(
        organization_id,
        workspace_id,
        request,
        status,
        &checks,
        resolved_content_sha256,
        runtime_manifest_sha256.as_deref(),
        runtime_healthy,
        provider_ready,
        browser_map_configured,
        input_binding_fingerprint.as_deref(),
        thread_status,
        input_status,
        analysis_status,
    );

    EvaluatedRunReadiness {
        readiness: RunReadiness {
            status,
            evaluation_fingerprint: fingerprint,
            checks,
            scope: Some(if request.purpose == RunStartPurpose::Conversation {
                ReadinessScope::Thread
            } else {
                ReadinessScope::Analysis
            }),
            thread_status: Some(thread_status),
            input_status: Some(input_status),
            analysis_status: Some(analysis_status),
        },
        agent: resolved_agent,
        supervisor_policy: resolved_policy,
    }
}

async fn analysis_binding_fingerprint(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    task_id: Uuid,
    workspace_id: Uuid,
) -> Option<String> {
    sqlx::query_scalar(
        "SELECT binding.fingerprint FROM task_dataset_bindings binding\
         JOIN workspace_dataset_releases release ON release.organization_id = binding.organization_id\
           AND release.id = binding.dataset_release_id\
         JOIN data_intake_sessions intake ON intake.organization_id = binding.organization_id\
           AND intake.id = binding.intake_session_id\
         WHERE binding.organization_id = $1 AND binding.task_id = $2\
           AND binding.workspace_id = $3 AND intake.task_id = binding.task_id\
           AND intake.workspace_id = binding.workspace_id\
           AND binding.contract_id = intake.contract_id\
           AND binding.contract_version = intake.contract_version\
           AND binding.contract_sha256 = intake.contract_sha256\
           AND intake.normalized_release_id = binding.dataset_release_id\
           AND release.workspace_id = binding.workspace_id\
           AND release.state = 'published' AND intake.status = 'ready'\
         LIMIT 1",
    )
    .bind(organization_id)
    .bind(task_id)
    .bind(workspace_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
}

pub(crate) async fn verify_agent_dependencies(
    git: &GitRuntime,
    workspace_id: Uuid,
    agent: &ResolvedAgentDefinition,
) -> Result<(), ()> {
    if agent
        .required_workspace_id()
        .is_some_and(|required| required != workspace_id)
    {
        return Err(());
    }
    if let Some(selection) = agent.capability_template.as_ref() {
        if selection.source
            == open_web_codex_platform_contracts::AgentCapabilityTemplateSource::WorkspacePackageRelease
        {
            let release_id = selection.release_id.ok_or(())?;
            if !git
                .capability_package_matches(
                    workspace_id,
                    &selection.definition_id,
                    &selection.version,
                    release_id,
                    &agent.capability_template_sha256,
                )
                .await
                .map_err(|_| ())?
            {
                return Err(());
            }
        }
    }
    for dataset in &agent.dataset_releases {
        if !git
            .dataset_release_matches(
                workspace_id,
                &dataset.dataset_id,
                &dataset.version,
                dataset.release_id,
                &dataset.content_sha256,
            )
            .await
            .map_err(|_| ())?
        {
            return Err(());
        }
    }
    Ok(())
}

pub(crate) async fn verify_supervisor_dependencies(
    git: &GitRuntime,
    workspace_id: Uuid,
    policy: &supervisor_policy::ResolvedSupervisorPolicy,
) -> Result<(), ()> {
    if policy
        .required_workspace_id
        .is_some_and(|required| required != workspace_id)
    {
        return Err(());
    }
    for package in &policy.workspace_capability_packages {
        if !git
            .capability_package_matches(
                workspace_id,
                &package.package_id,
                &package.version,
                package.release_id,
                &package.content_sha256,
            )
            .await
            .map_err(|_| ())?
        {
            return Err(());
        }
    }
    for dataset in &policy.dataset_releases {
        if !git
            .dataset_release_matches(
                workspace_id,
                &dataset.dataset_id,
                &dataset.version,
                dataset.release_id,
                &dataset.content_sha256,
            )
            .await
            .map_err(|_| ())?
        {
            return Err(());
        }
    }
    Ok(())
}

fn provider_is_available(catalog: &ProviderCatalog, request: &RunReadinessRequest) -> bool {
    !request.model_provider.trim().is_empty()
        && !request.model.trim().is_empty()
        && catalog.data.iter().any(|provider| {
            provider.id == request.model_provider
                && provider
                    .models
                    .iter()
                    .any(|model| model.model_id == request.model && model.show_in_picker)
        })
}

fn artifact_types_require_map<'a>(artifact_types: impl Iterator<Item = &'a str>) -> bool {
    artifact_types
        .into_iter()
        .any(|artifact_type| artifact_type == MAP_PRESENTATION_ARTIFACT_TYPE)
}

fn mcp_declarations_are_valid(servers: &[open_web_codex_adapter::RequiredMcpServer]) -> bool {
    servers.is_empty() || validate_required_mcp_servers(servers).is_ok()
}

fn check(
    code: RunReadinessCheckCode,
    status: RunReadinessStatus,
    message: &str,
    action: Option<RunReadinessAction>,
) -> RunReadinessCheck {
    RunReadinessCheck {
        code,
        status,
        message: message.to_string(),
        action,
    }
}

fn aggregate_status(checks: &[RunReadinessCheck]) -> RunReadinessStatus {
    if checks
        .iter()
        .any(|check| check.status == RunReadinessStatus::Blocked)
    {
        RunReadinessStatus::Blocked
    } else if checks
        .iter()
        .any(|check| check.status == RunReadinessStatus::Degraded)
    {
        RunReadinessStatus::Degraded
    } else {
        RunReadinessStatus::Ready
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluation_fingerprint(
    organization_id: Uuid,
    workspace_id: Uuid,
    request: &RunReadinessRequest,
    status: RunReadinessStatus,
    checks: &[RunReadinessCheck],
    resolved_content_sha256: Option<&str>,
    runtime_manifest_sha256: Option<&str>,
    runtime_healthy: bool,
    provider_ready: bool,
    browser_map_configured: bool,
    input_binding_fingerprint: Option<&str>,
    thread_status: RunReadinessStatus,
    input_status: RunReadinessStatus,
    analysis_status: RunReadinessStatus,
) -> String {
    let value = json!({
        "schemaVersion": "platform.run-readiness.v1",
        "organizationId": organization_id,
        "workspaceId": workspace_id,
        "request": request,
        "status": status,
        "checks": checks,
        "resolvedContentSha256": resolved_content_sha256,
        "runtimeManifestSha256": runtime_manifest_sha256,
        "runtimeHealthy": runtime_healthy,
        "providerReady": provider_ready,
        "browserMapConfigured": browser_map_configured,
        "inputBindingFingerprint": input_binding_fingerprint,
        "threadStatus": thread_status,
        "inputStatus": input_status,
        "analysisStatus": analysis_status,
    });
    hex::encode(Sha256::digest(
        serde_json::to_vec(&value).expect("readiness fingerprint input is serializable"),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        aggregate_status, artifact_types_require_map, mcp_declarations_are_valid,
        provider_is_available,
    };
    use open_web_codex_adapter::{CapabilityRootMcpInventory, RequiredMcpServer};
    use open_web_codex_platform_contracts::{
        ProviderCatalog, ProviderKind, ProviderModelSummary, ProviderSummary, RunReadinessAction,
        RunReadinessCheck, RunReadinessCheckCode, RunReadinessRequest, RunReadinessStatus,
        RunStartPurpose,
    };

    #[test]
    fn blocked_status_takes_precedence_over_degraded() {
        let checks = vec![
            RunReadinessCheck {
                code: RunReadinessCheckCode::MapPresentation,
                status: RunReadinessStatus::Degraded,
                message: "optional".to_string(),
                action: Some(RunReadinessAction::OpenMapsSettings),
            },
            RunReadinessCheck {
                code: RunReadinessCheckCode::ProviderModel,
                status: RunReadinessStatus::Blocked,
                message: "required".to_string(),
                action: Some(RunReadinessAction::OpenProviderSettings),
            },
        ];
        assert_eq!(aggregate_status(&checks), RunReadinessStatus::Blocked);
    }

    #[test]
    fn map_presentation_is_derived_from_the_artifact_contract() {
        assert!(artifact_types_require_map(
            ["report.v1", "map.v3"].into_iter()
        ));
        assert!(!artifact_types_require_map(
            ["geojson.v1", "indonesia_network_map.v1"].into_iter()
        ));
    }

    #[test]
    fn mcp_readiness_ignores_workspace_manifest_presence_and_content() {
        let manifest_root = tempfile::tempdir().expect("temp dir");
        let manifest = manifest_root.path().join(".mcp.json");
        let required = vec![RequiredMcpServer {
            name: "delivery_promise".to_string(),
            tools: vec!["check_promises".to_string()],
            capability_roots: vec![CapabilityRootMcpInventory {
                capability_root_id: "reviewed-delivery-capability".to_string(),
                mcp_server_names: vec!["delivery_promise".to_string()],
            }],
        }];

        assert!(mcp_declarations_are_valid(&required));

        std::fs::write(
            &manifest,
            r#"{"mcpServers":{"hidden_admin":{"command":"./admin"}}}"#,
        )
        .expect("write a conflicting workspace manifest");
        assert!(mcp_declarations_are_valid(&required));

        std::fs::remove_file(manifest).expect("remove workspace manifest");
        assert!(mcp_declarations_are_valid(&required));
    }

    #[test]
    fn provider_readiness_requires_the_exact_discovered_model() {
        let request = RunReadinessRequest {
            model_provider: "provider-a".to_string(),
            model: "model-a".to_string(),
            supervisor_policy: None,
            agent: None,
            fork_thread_id: None,
            fork_source_run_id: None,
            purpose: RunStartPurpose::Analysis,
            task_id: None,
        };
        let provider = ProviderSummary {
            id: "provider-a".to_string(),
            name: "Provider A".to_string(),
            base_url: None,
            env_key: None,
            wire_api: "responses".to_string(),
            kind: ProviderKind::Custom,
            is_current: true,
            model_count: 0,
            can_edit: true,
            can_delete: true,
            can_fetch_models: true,
            models: Vec::new(),
        };
        let empty_catalog = ProviderCatalog {
            data: vec![provider.clone()],
            current_provider_id: provider.id.clone(),
            current_model_id: None,
        };
        assert!(!provider_is_available(&empty_catalog, &request));

        let exact_catalog = ProviderCatalog {
            data: vec![ProviderSummary {
                model_count: 1,
                models: vec![ProviderModelSummary {
                    model_id: "model-a".to_string(),
                    model_name: Some("Model A".to_string()),
                    max_token_len: None,
                    max_output_tokens: None,
                    show_in_picker: true,
                    context_window: None,
                }],
                ..provider
            }],
            current_provider_id: "provider-a".to_string(),
            current_model_id: Some("model-a".to_string()),
        };
        assert!(provider_is_available(&exact_catalog, &request));
    }
}
