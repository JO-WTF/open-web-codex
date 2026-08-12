ALTER TABLE profile_provider_definitions
    ADD COLUMN credential_env_key TEXT NULL;

ALTER TABLE profile_provider_definitions
    ADD CONSTRAINT profile_provider_definitions_credential_env_key_format
    CHECK (
        credential_env_key IS NULL
        OR credential_env_key ~ '^[A-Z0-9_]+$'
    );
