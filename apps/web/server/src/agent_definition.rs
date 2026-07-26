use open_web_codex_platform_contracts::AgentDefinitionSummary;
use serde::Deserialize;
use thiserror::Error;

const DATA_AGENT: &str = include_str!("../resources/agent-definitions/data-agent-v1.json");
const NETWORK_PLANNING_AGENT: &str =
    include_str!("../resources/agent-definitions/network-planning-agent-v1.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedAgentDefinition {
    definition_id: String,
    version: String,
    display_name: String,
    description: String,
    runtime_role: String,
    responsibilities: Vec<String>,
    input_artifact_types: Vec<String>,
    output_artifact_types: Vec<String>,
    required_capabilities: Vec<String>,
    risks: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum AgentDefinitionError {
    #[error("published Agent Definition is invalid")]
    Invalid,
}

pub(crate) fn list_published() -> Result<Vec<AgentDefinitionSummary>, AgentDefinitionError> {
    [DATA_AGENT, NETWORK_PLANNING_AGENT]
        .into_iter()
        .map(parse_definition)
        .map(|definition| definition.map(Into::into))
        .collect()
}

pub(crate) fn enterprise_runtime_roles() -> Result<Vec<String>, AgentDefinitionError> {
    Ok(list_published()?
        .into_iter()
        .map(|definition| definition.runtime_role)
        .collect())
}

fn parse_definition(source: &str) -> Result<PublishedAgentDefinition, AgentDefinitionError> {
    let definition = serde_json::from_str::<PublishedAgentDefinition>(source)
        .map_err(|_| AgentDefinitionError::Invalid)?;
    for value in [
        &definition.definition_id,
        &definition.version,
        &definition.display_name,
        &definition.description,
        &definition.runtime_role,
    ] {
        if value.trim().is_empty() || value.len() > 256 {
            return Err(AgentDefinitionError::Invalid);
        }
    }
    if !definition.runtime_role.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
    }) {
        return Err(AgentDefinitionError::Invalid);
    }
    for values in [
        &definition.responsibilities,
        &definition.output_artifact_types,
        &definition.required_capabilities,
        &definition.risks,
    ] {
        if values.is_empty()
            || values
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 512)
        {
            return Err(AgentDefinitionError::Invalid);
        }
    }
    if definition
        .input_artifact_types
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > 128)
    {
        return Err(AgentDefinitionError::Invalid);
    }
    Ok(definition)
}

impl From<PublishedAgentDefinition> for AgentDefinitionSummary {
    fn from(value: PublishedAgentDefinition) -> Self {
        Self {
            definition_id: value.definition_id,
            version: value.version,
            display_name: value.display_name,
            description: value.description,
            runtime_role: value.runtime_role,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enterprise_definitions_resolve_to_exact_runtime_roles() {
        let definitions = list_published().unwrap();
        assert_eq!(definitions.len(), 2);
        assert_eq!(
            enterprise_runtime_roles().unwrap(),
            vec![
                "data_agent".to_string(),
                "network_planning_agent".to_string()
            ]
        );
    }

    #[test]
    fn network_definition_declares_the_actual_planning_artifact_contracts() {
        let definition = parse_definition(NETWORK_PLANNING_AGENT).unwrap();
        assert_eq!(
            definition.output_artifact_types,
            vec![
                "network_snapshot.v1",
                "route_matrix.v1",
                "current_coverage_result.v1",
                "network_scenario_result.v1",
                "scenario_comparison.v1",
                "facility_location_solution.v1",
            ]
        );
        assert!(!definition
            .responsibilities
            .iter()
            .any(|responsibility| responsibility.contains("network-simulation.v1")));
        assert!(definition
            .required_capabilities
            .contains(&"supply_chain_planner.evaluate_current_coverage".to_string()));
    }
}
