CREATE TABLE IF NOT EXISTS profile_provider_definitions (
    profile_id         UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    provider_id        TEXT NOT NULL,
    name               TEXT NOT NULL,
    base_url           TEXT NOT NULL,
    wire_api           TEXT NOT NULL CHECK (wire_api IN ('chat', 'responses')),
    models             JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_selected        BOOLEAN NOT NULL DEFAULT FALSE,
    selected_model_id  TEXT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (profile_id, provider_id),
    CHECK (jsonb_typeof(models) = 'array')
);

CREATE INDEX IF NOT EXISTS idx_profile_provider_definitions_selected
    ON profile_provider_definitions(profile_id, is_selected);
