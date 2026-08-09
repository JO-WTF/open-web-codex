-- Runtime-native Agent collaboration and MCP/approval lifecycles are the
-- only control plane. Remove the superseded platform-owned work state and
-- Supervisor continuation/snapshot tables in dependency order.

DROP TABLE work_operation_inputs;
DROP TABLE work_operation_outputs;
DROP TABLE work_state_events;
DROP TABLE work_deliverables;
DROP TABLE work_blocking_inputs;
DROP TABLE work_component_dependencies;
DROP TABLE work_components;
DROP TABLE work_operations;
DROP TABLE work_states;
DROP TABLE work_state_definitions;

DROP TABLE supervisor_policy_bindings;
DROP TABLE supervisor_run_continuations;
DROP TABLE supervisor_policy_snapshots;
DROP TABLE supervisor_releases;
DROP TABLE supervisor_revisions;
DROP TABLE supervisor_definitions;

ALTER TABLE provider_call_metrics
    DROP COLUMN stable_prefix_sha256,
    DROP COLUMN tool_inventory_sha256,
    DROP COLUMN skill_set_sha256,
    DROP COLUMN runtime_role_sha256;
