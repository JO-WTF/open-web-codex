ALTER TABLE profile_provider_definitions
    ADD COLUMN supports_function_tools BOOLEAN NOT NULL DEFAULT FALSE;
