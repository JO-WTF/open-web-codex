-- A terminal Runtime Thread may receive a later user Turn. That is a new
-- platform Run attempt, while the original Run remains immutable terminal
-- history. Persist the direct source and last exact Turn so idempotency is
-- exact even when several attempts reuse the same Runtime Thread.

ALTER TABLE runs
    ADD COLUMN continued_from_run_id UUID REFERENCES runs(id) ON DELETE RESTRICT;

ALTER TABLE runs
    ADD COLUMN last_turn_id TEXT;

ALTER TABLE runs
    ADD CONSTRAINT runs_continued_from_run_check CHECK (
        continued_from_run_id IS NULL OR continued_from_run_id <> id
    );

ALTER TABLE runs
    ADD CONSTRAINT runs_last_turn_id_check CHECK (
        last_turn_id IS NULL OR length(last_turn_id) BETWEEN 1 AND 256
    );

CREATE INDEX idx_runs_continued_from
    ON runs(continued_from_run_id)
    WHERE continued_from_run_id IS NOT NULL;
