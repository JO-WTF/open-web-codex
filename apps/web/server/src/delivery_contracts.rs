use jsonschema::{Draft, JSONSchema};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ContentVerifier {
    JsonSchema { document: Value },
    MarkdownMarker { marker: String },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DeliveryKind {
    WorkspaceArtifact { verifier: ContentVerifier },
    InlineGeoJsonMapCard,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DeliveryContract {
    pub id: String,
    pub server: String,
    pub tool: String,
    pub kind: DeliveryKind,
    pub schema: String,
    pub mime_type: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DeliveryRegistry {
    producers: BTreeMap<(String, String), DeliveryContract>,
}

impl DeliveryRegistry {
    pub(crate) fn new(contracts: Vec<DeliveryContract>) -> Result<Self, String> {
        let mut producers = BTreeMap::new();
        for contract in contracts {
            let key = (contract.server.clone(), contract.tool.clone());
            if producers.insert(key.clone(), contract).is_some() {
                return Err(format!("duplicate delivery producer {}/{}", key.0, key.1));
            }
        }
        Ok(Self { producers })
    }

    pub(crate) fn for_item(&self, item: &Map<String, Value>) -> Option<&DeliveryContract> {
        let server = item.get("server")?.as_str()?;
        let tool = item.get("tool")?.as_str()?;
        self.producers.get(&(server.to_string(), tool.to_string()))
    }

    pub(crate) fn merge(registries: impl IntoIterator<Item = Self>) -> Result<Self, String> {
        let mut producers = BTreeMap::new();
        for registry in registries {
            for (key, contract) in registry.producers {
                if let Some(existing) = producers.get(&key) {
                    if existing != &contract {
                        return Err(format!("conflicting delivery producer {}/{}", key.0, key.1));
                    }
                    continue;
                }
                producers.insert(key, contract);
            }
        }
        let merged = Self { producers };
        merged.validate()?;
        Ok(merged)
    }

    pub(crate) fn workspace_by_schema_mime(
        &self,
        schema: &str,
        mime_type: &str,
    ) -> Option<&DeliveryContract> {
        self.producers.values().find(|contract| {
            matches!(contract.kind, DeliveryKind::WorkspaceArtifact { .. })
                && contract.schema == schema
                && contract.mime_type == mime_type
        })
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let mut workspace_shapes = BTreeMap::new();
        for contract in self.producers.values() {
            if contract.id.is_empty()
                || contract.server.is_empty()
                || contract.tool.is_empty()
                || contract.schema.is_empty()
                || contract.mime_type.is_empty()
                || contract.display_name.is_empty()
            {
                return Err("delivery contract contains an empty required field".to_string());
            }
            match &contract.kind {
                DeliveryKind::WorkspaceArtifact { verifier } => {
                    let shape = (contract.schema.clone(), contract.mime_type.clone());
                    if workspace_shapes
                        .insert(shape, contract.id.clone())
                        .is_some()
                    {
                        return Err(
                            "workspace delivery schema and media type must be unique".into()
                        );
                    }
                    match verifier {
                        ContentVerifier::JsonSchema { document } => {
                            JSONSchema::options()
                                .with_draft(Draft::Draft202012)
                                .compile(document)
                                .map_err(|_| "delivery JSON Schema is invalid".to_string())?;
                        }
                        ContentVerifier::MarkdownMarker { marker } => {
                            if marker.is_empty()
                                || marker.len() > 256
                                || marker.contains(['\n', '\r'])
                            {
                                return Err("delivery Markdown marker is invalid".to_string());
                            }
                        }
                    }
                }
                DeliveryKind::InlineGeoJsonMapCard => {
                    if contract.mime_type != "application/vnd.open-web-codex.map-card+json" {
                        return Err("inline map card media type is invalid".to_string());
                    }
                }
            }
        }
        Ok(())
    }
}

impl ContentVerifier {
    pub(crate) fn snapshot(&self) -> Value {
        match self {
            Self::JsonSchema { document } => serde_json::json!({
                "kind": "json_schema",
                "document": document,
            }),
            Self::MarkdownMarker { marker } => serde_json::json!({
                "kind": "markdown_marker",
                "marker": marker,
            }),
        }
    }

    pub(crate) fn from_snapshot(snapshot: &Value) -> Result<Self, &'static str> {
        let object = snapshot
            .as_object()
            .ok_or("artifact_verifier_snapshot_invalid")?;
        match object.get("kind").and_then(Value::as_str) {
            Some("json_schema") if object.len() == 2 => {
                let document = object
                    .get("document")
                    .filter(|document| document.is_object())
                    .cloned()
                    .ok_or("artifact_verifier_snapshot_invalid")?;
                Ok(Self::JsonSchema { document })
            }
            Some("markdown_marker") if object.len() == 2 => {
                let marker = object
                    .get("marker")
                    .and_then(Value::as_str)
                    .filter(|marker| {
                        !marker.is_empty() && marker.len() <= 256 && !marker.contains(['\n', '\r'])
                    })
                    .ok_or("artifact_verifier_snapshot_invalid")?;
                Ok(Self::MarkdownMarker {
                    marker: marker.to_string(),
                })
            }
            _ => Err("artifact_verifier_snapshot_invalid"),
        }
    }
}

#[cfg(test)]
pub(crate) fn warehouse_test_registry() -> DeliveryRegistry {
    let schema = serde_json::from_str(include_str!(
        "../../../../tools/warehouse-network-planner/contracts/schemas/network_comparison_map_bundle.v2.schema.json"
    ))
    .expect("warehouse map delivery schema");
    DeliveryRegistry::new(vec![
        DeliveryContract {
            id: "network-comparison-map".into(),
            server: "supply_chain".into(),
            tool: "render_network_comparison_map".into(),
            kind: DeliveryKind::WorkspaceArtifact {
                verifier: ContentVerifier::JsonSchema { document: schema },
            },
            schema: "network_comparison_map_bundle.v2".into(),
            mime_type: "application/json".into(),
            display_name: "Warehouse network: before vs after comparison".into(),
        },
        DeliveryContract {
            id: "network-planning-report".into(),
            server: "supply_chain".into(),
            tool: "publish_network_planning_report".into(),
            kind: DeliveryKind::WorkspaceArtifact {
                verifier: ContentVerifier::MarkdownMarker {
                    marker: "<!-- network_planning_report_markdown.v2 -->".into(),
                },
            },
            schema: "network_planning_report_markdown.v2".into(),
            mime_type: "text/markdown".into(),
            display_name: "Warehouse network planning report".into(),
        },
        DeliveryContract {
            id: "network-map-card".into(),
            server: "map_utils".into(),
            tool: "create_network_map_card".into(),
            kind: DeliveryKind::InlineGeoJsonMapCard,
            schema: "map.v3".into(),
            mime_type: "application/vnd.open-web-codex.map-card+json".into(),
            display_name: "Warehouse network map".into(),
        },
        DeliveryContract {
            id: "network-map-card-custom".into(),
            server: "map_utils".into(),
            tool: "create_map_card".into(),
            kind: DeliveryKind::InlineGeoJsonMapCard,
            schema: "map.v3".into(),
            mime_type: "application/vnd.open-web-codex.map-card+json".into(),
            display_name: "Custom warehouse map".into(),
        },
        DeliveryContract {
            id: "network-map-card-revision".into(),
            server: "map_utils".into(),
            tool: "revise_map_card".into(),
            kind: DeliveryKind::InlineGeoJsonMapCard,
            schema: "map.v3".into(),
            mime_type: "application/vnd.open-web-codex.map-card+json".into(),
            display_name: "Revised interactive map".into(),
        },
    ])
    .expect("warehouse delivery registry")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_producer_selects_only_its_declared_delivery() {
        let registry = warehouse_test_registry();
        let matching = serde_json::json!({
            "server": "map_utils",
            "tool": "create_network_map_card"
        });
        let unrelated = serde_json::json!({
            "server": "map_utils",
            "tool": "get_route"
        });
        assert!(matches!(
            registry
                .for_item(matching.as_object().expect("item"))
                .map(|contract| &contract.kind),
            Some(DeliveryKind::InlineGeoJsonMapCard)
        ));
        assert!(registry
            .for_item(unrelated.as_object().expect("item"))
            .is_none());
    }

    #[test]
    fn duplicate_producer_is_rejected() {
        let registry = warehouse_test_registry();
        let first = registry
            .for_item(
                serde_json::json!({"server":"supply_chain","tool":"publish_network_planning_report"})
                    .as_object()
                    .expect("item"),
            )
            .expect("delivery")
            .clone();
        assert!(DeliveryRegistry::new(vec![first.clone(), first]).is_err());
    }

    #[test]
    fn package_registries_share_identical_producers_but_reject_contract_drift() {
        let first = warehouse_test_registry();
        let second = warehouse_test_registry();
        let merged = DeliveryRegistry::merge([first, second]).expect("merge shared deliveries");
        assert!(merged
            .for_item(
                serde_json::json!({"server":"supply_chain","tool":"publish_network_planning_report"})
                    .as_object()
                    .expect("item"),
            )
            .is_some());

        let mut drifted = warehouse_test_registry()
            .for_item(
                serde_json::json!({"server":"supply_chain","tool":"publish_network_planning_report"})
                    .as_object()
                    .expect("item"),
            )
            .expect("delivery")
            .clone();
        drifted.display_name = "Drifted report".to_string();
        let conflict = DeliveryRegistry::new(vec![drifted]).expect("drifted registry");
        assert!(DeliveryRegistry::merge([warehouse_test_registry(), conflict]).is_err());
    }
}
