 -- Organizations and memberships.
 -- Depends on: 20260712000002_auth.sql

 CREATE TABLE IF NOT EXISTS organizations (
     id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
     name            TEXT NOT NULL,
     slug            TEXT NOT NULL UNIQUE,
     created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
     updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
 );

 CREATE TABLE IF NOT EXISTS memberships (
     id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
     organization_id   UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
     user_id           UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
     role              TEXT NOT NULL DEFAULT 'member'
                           CHECK (role IN ('owner', 'admin', 'member')),
     created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
     UNIQUE(organization_id, user_id)
 );
 CREATE INDEX IF NOT EXISTS idx_memberships_org ON memberships(organization_id);
 CREATE INDEX IF NOT EXISTS idx_memberships_user ON memberships(user_id);
