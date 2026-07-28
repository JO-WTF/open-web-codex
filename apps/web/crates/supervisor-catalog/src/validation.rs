pub(crate) const MAX_DEFINITION_ID_BYTES: usize = 96;
pub(crate) const MAX_VERSION_BYTES: usize = 64;
pub(crate) const MAX_RUNTIME_ROLE_NAME_BYTES: usize = 64;

pub(crate) fn is_safe_capability_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

pub(crate) fn is_safe_definition_id(value: &str) -> bool {
    is_safe_path_segment(value, MAX_DEFINITION_ID_BYTES, false)
}

pub(crate) fn is_safe_version(value: &str) -> bool {
    is_safe_path_segment(value, MAX_VERSION_BYTES, true)
}

pub(crate) fn is_safe_runtime_role_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_RUNTIME_ROLE_NAME_BYTES
        && !matches!(
            value,
            "default"
                | "allowed_roles"
                | "enabled"
                | "max_concurrent_threads_per_session"
                | "max_depth"
                | "default_subagent_model"
                | "default_subagent_reasoning_effort"
                | "interrupt_message"
                | "job_max_runtime_seconds"
        )
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

pub(crate) fn is_safe_artifact_type(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.')
        })
        && value.contains('.')
}

fn is_safe_path_segment(value: &str, maximum_bytes: usize, allow_period: bool) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value != "."
        && value != ".."
        && !value.contains("..")
        && value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == b'-'
                || byte == b'_'
                || (allow_period && byte == b'.')
        })
}
