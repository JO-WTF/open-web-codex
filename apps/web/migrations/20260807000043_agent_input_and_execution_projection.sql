ALTER TABLE runtime_agent_execution_projections
    ADD COLUMN display_title TEXT NOT NULL DEFAULT 'Agent task',
    ADD COLUMN result_summary TEXT,
    ADD COLUMN waiting_approval_id UUID REFERENCES approvals(id) ON DELETE SET NULL,
    ADD COLUMN wait_started_at TIMESTAMPTZ,
    ADD COLUMN wait_cycle_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN terminal_sequence BIGINT;

ALTER TABLE runtime_agent_execution_projections
    DROP CONSTRAINT IF EXISTS runtime_agent_execution_projections_status_check;

ALTER TABLE runtime_agent_execution_projections
    ADD CONSTRAINT runtime_agent_execution_projections_status_check
    CHECK (status IN (
        'pending', 'running', 'waiting', 'waiting_for_input',
        'completed', 'failed', 'interrupted'
    ));

ALTER TABLE runtime_agent_execution_projections
    ADD CONSTRAINT runtime_agent_execution_projections_display_title_check
    CHECK (length(display_title) BETWEEN 1 AND 80);

ALTER TABLE runtime_agent_execution_projections
    ADD CONSTRAINT runtime_agent_execution_projections_result_summary_check
    CHECK (result_summary IS NULL OR length(result_summary) BETWEEN 1 AND 1000);

ALTER TABLE runtime_agent_execution_projections
    ADD CONSTRAINT runtime_agent_execution_projections_wait_cycle_count_check
    CHECK (wait_cycle_count >= 0);

DROP INDEX IF EXISTS idx_runtime_agent_execution_projections_active;

CREATE INDEX idx_runtime_agent_execution_projections_active
    ON runtime_agent_execution_projections(root_run_id, agent_thread_id, ordinal DESC)
    WHERE status IN ('pending', 'running', 'waiting', 'waiting_for_input');

CREATE INDEX idx_runtime_agent_execution_projections_waiting_input
    ON runtime_agent_execution_projections(root_run_id, waiting_approval_id)
    WHERE status = 'waiting_for_input';
