-- Phase one uses ordinary files in an authorized Workspace as its only shared
-- data surface. These thread-first intake, SourceAsset, Dataset Release, and
-- Task binding objects have no owner in the current architecture.

-- The repository warehouse Supervisor/Agent/Tutorial catalog was an earlier
-- orchestration prototype coupled to this data plane. Its checked-in packages
-- are removed with this migration; clean environments must not retain the
-- seeded mutable Supervisor Draft that referenced those packages.
DELETE FROM supervisor_definitions
WHERE policy_id = 'enterprise-supervisor-copilot';

DROP TABLE task_analysis_execution_snapshots;
DROP TABLE task_intake_artifact_projections;
DROP TABLE task_policy_agent_producers;
DROP TABLE data_intake_input_requests;

ALTER TABLE data_intake_sessions
    DROP CONSTRAINT data_intake_sessions_current_binding_fk;

DROP TABLE task_dataset_bindings;
DROP TABLE data_intake_sessions;

DROP TABLE workspace_data_draft_assets;
DROP TABLE workspace_data_drafts;
DROP TABLE workspace_data_source_assets;

DROP TABLE agent_release_dataset_dependencies;
DROP TABLE workspace_dataset_release_files;
DROP TABLE workspace_dataset_releases;

ALTER TABLE artifacts DROP COLUMN intake_envelope;
ALTER TABLE workspaces DROP COLUMN source_revision;
