 -- Auth tables: users, sessions, and bootstrap tokens.
 -- Depends on: 20260712000001_initial.sql

 -- Users (participants in organizations)
 CREATE TABLE IF NOT EXISTS users (
     id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
     name            TEXT NOT NULL,
     email           TEXT NOT NULL UNIQUE,
     password_hash   TEXT NOT NULL,
     role            TEXT NOT NULL DEFAULT 'member'
                         CHECK (role IN ('owner', 'admin', 'member')),
     created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
     updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
 );
 CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

 -- Sessions (login sessions)
 CREATE TABLE IF NOT EXISTS sessions (
     id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
     user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
     token_hash      TEXT NOT NULL UNIQUE,
     expires_at      TIMESTAMPTZ NOT NULL,
     created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
     revoked_at      TIMESTAMPTZ
 );
 CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token_hash);
 CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);

 -- Bootstrap tokens (one-time setup)
 CREATE TABLE IF NOT EXISTS bootstrap_tokens (
     id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
     token           TEXT NOT NULL UNIQUE,
     consumed_at     TIMESTAMPTZ,
     created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
 );
