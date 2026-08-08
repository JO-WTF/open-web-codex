ALTER TABLE runtime_agent_execution_projections
    DROP CONSTRAINT IF EXISTS runtime_agent_execution_projections_status_check;

ALTER TABLE runtime_agent_execution_projections
    ADD CONSTRAINT runtime_agent_execution_projections_status_check
    CHECK (status IN (
        'pending', 'running', 'waiting', 'waiting_for_input',
        'completed', 'failed', 'rejected', 'cancelled', 'timeout', 'interrupted'
    ));

DROP INDEX IF EXISTS idx_runtime_agent_execution_projections_active;

CREATE INDEX idx_runtime_agent_execution_projections_active
    ON runtime_agent_execution_projections(root_run_id, agent_thread_id, ordinal DESC)
    WHERE status IN ('pending', 'running', 'waiting', 'waiting_for_input');
