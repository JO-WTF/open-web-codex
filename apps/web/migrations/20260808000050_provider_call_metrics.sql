-- Bounded provider-call observability. Prompt and completion bodies are not
-- stored; NULL means the Runtime/provider did not report a field.

CREATE TABLE provider_call_metrics (
    id                         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id            UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    profile_id                 UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    run_id                     UUID REFERENCES runs(id) ON DELETE SET NULL,
    provider_id                TEXT NOT NULL,
    model_id                  TEXT NOT NULL,
    input_tokens               BIGINT,
    cached_input_tokens        BIGINT,
    output_tokens              BIGINT,
    tool_schema_tokens         BIGINT,
    latency_ms                 BIGINT,
    first_token_ms             BIGINT,
    compaction_count           INTEGER NOT NULL DEFAULT 0,
    terminal_status            TEXT NOT NULL,
    stable_prefix_sha256       TEXT CHECK (stable_prefix_sha256 IS NULL OR stable_prefix_sha256 ~ '^[0-9a-f]{64}$'),
    tool_inventory_sha256      TEXT CHECK (tool_inventory_sha256 IS NULL OR tool_inventory_sha256 ~ '^[0-9a-f]{64}$'),
    skill_set_sha256            TEXT CHECK (skill_set_sha256 IS NULL OR skill_set_sha256 ~ '^[0-9a-f]{64}$'),
    runtime_role_sha256         TEXT CHECK (runtime_role_sha256 IS NULL OR runtime_role_sha256 ~ '^[0-9a-f]{64}$'),
    source_event_sequence       BIGINT,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, source_event_sequence)
);

CREATE INDEX provider_call_metrics_run_idx
    ON provider_call_metrics(organization_id, run_id, created_at DESC);
CREATE INDEX provider_call_metrics_profile_idx
    ON provider_call_metrics(organization_id, profile_id, created_at DESC);
