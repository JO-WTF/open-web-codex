-- Inline map cards are a bounded browser projection of one exact completed
-- map_utils/create_map_card Item. The renderer keeps provider-owned Resource
-- references; it never copies Resource bytes into Platform persistence or the
-- durable final Artifact tables.

CREATE TABLE inline_map_cards (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    run_id              UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    producer_thread_id  TEXT NOT NULL,
    producer_turn_id    TEXT NOT NULL,
    producer_item_id    TEXT NOT NULL,
    card_ref            TEXT NOT NULL,
    renderer_payload    JSONB NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, card_ref),
    UNIQUE (run_id, producer_thread_id, producer_turn_id, producer_item_id)
);

CREATE INDEX idx_inline_map_cards_run
    ON inline_map_cards(organization_id, run_id, created_at);
