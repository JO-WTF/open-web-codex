 -- Run events log for event replay after server restart.
 -- Depends on: 20260712000001_initial.sql

 CREATE TABLE IF NOT EXISTS run_events (
     id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
     run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
     event_type      TEXT NOT NULL,
     payload         JSONB NOT NULL DEFAULT '{}',
     created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
 );

 CREATE INDEX IF NOT EXISTS idx_run_events_run ON run_events(run_id);
 CREATE INDEX IF NOT EXISTS idx_run_events_created ON run_events(created_at DESC);
