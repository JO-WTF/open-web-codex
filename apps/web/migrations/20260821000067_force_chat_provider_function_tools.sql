UPDATE profile_provider_definitions
SET supports_function_tools = TRUE,
    updated_at = now()
WHERE wire_api = 'chat'
  AND supports_function_tools = FALSE;

ALTER TABLE profile_provider_definitions
    ADD CONSTRAINT profile_provider_definitions_chat_function_tools
    CHECK (wire_api <> 'chat' OR supports_function_tools);
