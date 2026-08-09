-- Catalog, Studio, Python capability publication, governed Agent startup, and
-- DB-only installation/readiness no longer have a production owner. The
-- current phase-one Runtime path uses Profile-native Skill/Role/MCP discovery.

DROP TABLE capability_catalog_installation_events;
DROP TABLE capability_catalog_installations;
DROP TABLE capability_catalog_release_dependencies;
DROP TABLE capability_catalog_releases;
DROP TABLE capability_catalog_drafts;

DROP TABLE agent_run_bindings;
DROP TABLE agent_run_snapshots;
DROP TABLE supervisor_release_agent_dependencies;
DROP TABLE agent_release_capability_package_dependencies;
DROP TABLE agent_definition_releases;
DROP TABLE agent_definition_revisions;
DROP TABLE agent_definitions;
DROP TABLE workspace_capability_package_releases;

DROP TABLE supervisor_instruction_policy_releases;
