-- Current inline map-card contract: the renderer projection remains browser-only
-- while Maps owns the reusable presentation spec as an exact MCP Resource.
ALTER TABLE inline_map_cards
    ADD COLUMN map_spec_ref JSONB NOT NULL;
