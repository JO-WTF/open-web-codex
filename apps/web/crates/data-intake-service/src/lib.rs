//! Pure, bounded Data Intake contracts shared by HTTP and future SDK callers.
//!
//! SQL persistence, authorization and Dataset ownership stay in the Platform
//! Server.  This crate owns only validation rules that must not be reimplemented
//! by each transport.

use open_web_codex_platform_contracts::{DataIntakeParameterAnswer, DataRequirementParameter};
use serde_json::Value;
use std::collections::HashSet;
use thiserror::Error;

pub const MAX_JSON_DEPTH: usize = 64;
pub const MAX_JSON_NODES: usize = 250_000;
pub const MAX_XLSX_ENTRIES: u32 = 2_048;
pub const MAX_XLSX_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("{message}")]
pub struct DataIntakeValidationError {
    message: String,
}

impl DataIntakeValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub fn validate_parameter_answers(
    parameters: &[DataRequirementParameter],
    answers: &[DataIntakeParameterAnswer],
) -> Result<Vec<DataIntakeParameterAnswer>, DataIntakeValidationError> {
    let mut seen = HashSet::new();
    for answer in answers {
        let parameter = parameters
            .iter()
            .find(|parameter| parameter.name == answer.name)
            .ok_or_else(|| {
                DataIntakeValidationError::new(
                    "The submitted parameter is not in the published requirement Profile",
                )
            })?;
        if answer.source.trim().is_empty()
            || answer.value.is_null()
            || !seen.insert(answer.name.clone())
        {
            return Err(DataIntakeValidationError::new(format!(
                "Parameter {} requires a non-empty source and one value",
                parameter.display_name
            )));
        }
        if answer.unit.as_deref() != parameter.unit.as_deref() {
            return Err(DataIntakeValidationError::new(format!(
                "Parameter {} must use unit {:?}",
                parameter.display_name, parameter.unit
            )));
        }
        if parameter.data_type == "number"
            && answer
                .value
                .as_f64()
                .is_none_or(|value| !value.is_finite())
        {
            return Err(DataIntakeValidationError::new(format!(
                "Parameter {} must be a finite number",
                parameter.display_name
            )));
        }
        if parameter.data_type == "string"
            && answer
                .value
                .as_str()
                .is_some_and(|value| value.trim().is_empty())
        {
            return Err(DataIntakeValidationError::new(format!(
                "Parameter {} must not be empty",
                parameter.display_name
            )));
        }
    }
    Ok(answers.to_vec())
}

pub fn validate_file_name(file_name: &str) -> Result<(), DataIntakeValidationError> {
    if file_name.len() > 256
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name == "."
        || file_name == ".."
    {
        return Err(DataIntakeValidationError::new(
            "File names must be simple relative names",
        ));
    }
    if file_name.to_ascii_lowercase().ends_with(".xls") {
        return Err(DataIntakeValidationError::new(
            "Legacy .xls files are not supported; export as .xlsx, .csv or .json",
        ));
    }
    Ok(())
}

pub fn normalize_file_name(file_name: &str) -> String {
    file_name.trim().to_ascii_lowercase()
}

pub fn media_type_for(file_name: &str) -> Result<String, DataIntakeValidationError> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".xlsx") {
        Ok("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string())
    } else if lower.ends_with(".csv") {
        Ok("text/csv".to_string())
    } else if lower.ends_with(".json") {
        Ok("application/json".to_string())
    } else {
        Err(DataIntakeValidationError::new(
            "Only .xlsx, .csv and .json files are supported",
        ))
    }
}

pub fn validate_content(file_name: &str, bytes: &[u8]) -> Result<(), DataIntakeValidationError> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".json") {
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|_| DataIntakeValidationError::new("JSON input is invalid"))?;
        let mut count = 0;
        validate_json_shape(&value, 0, &mut count)?;
    } else if lower.ends_with(".xlsx") {
        validate_xlsx_archive(bytes)?;
    } else if bytes.iter().filter(|byte| **byte == b'\n').count() > 1_000_000 {
        return Err(DataIntakeValidationError::new("CSV contains too many rows"));
    }
    Ok(())
}

pub fn validate_xlsx_archive(bytes: &[u8]) -> Result<(), DataIntakeValidationError> {
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err(DataIntakeValidationError::new("The XLSX file is invalid"));
    }
    let search_start = bytes.len().saturating_sub(65_557);
    let eocd = bytes[search_start..]
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .map(|offset| search_start + offset)
        .ok_or_else(|| DataIntakeValidationError::new("The XLSX central directory is missing"))?;
    if eocd + 22 > bytes.len() {
        return Err(DataIntakeValidationError::new(
            "The XLSX central directory is truncated",
        ));
    }
    let entries = u16::from_le_bytes([bytes[eocd + 10], bytes[eocd + 11]]) as u32;
    let central_size = u32::from_le_bytes([
        bytes[eocd + 12],
        bytes[eocd + 13],
        bytes[eocd + 14],
        bytes[eocd + 15],
    ]) as usize;
    let central_offset = u32::from_le_bytes([
        bytes[eocd + 16],
        bytes[eocd + 17],
        bytes[eocd + 18],
        bytes[eocd + 19],
    ]) as usize;
    if entries == 0
        || entries > MAX_XLSX_ENTRIES
        || central_offset
            .checked_add(central_size)
            .is_none_or(|end| end > bytes.len())
    {
        return Err(DataIntakeValidationError::new(
            "The XLSX archive is too large or unsupported",
        ));
    }
    let central_end = central_offset + central_size;
    let mut cursor = central_offset;
    let mut expanded_bytes = 0u64;
    for _ in 0..entries {
        if cursor.checked_add(46).is_none_or(|end| end > central_end)
            || &bytes[cursor..cursor + 4] != b"PK\x01\x02"
        {
            return Err(DataIntakeValidationError::new(
                "The XLSX central directory is invalid",
            ));
        }
        let compressed = u32::from_le_bytes([
            bytes[cursor + 20],
            bytes[cursor + 21],
            bytes[cursor + 22],
            bytes[cursor + 23],
        ]) as u64;
        let expanded = u32::from_le_bytes([
            bytes[cursor + 24],
            bytes[cursor + 25],
            bytes[cursor + 26],
            bytes[cursor + 27],
        ]) as u64;
        let name_len = u16::from_le_bytes([bytes[cursor + 28], bytes[cursor + 29]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[cursor + 30], bytes[cursor + 31]]) as usize;
        let comment_len = u16::from_le_bytes([bytes[cursor + 32], bytes[cursor + 33]]) as usize;
        let record_end = cursor
            .checked_add(46)
            .and_then(|end| end.checked_add(name_len))
            .and_then(|end| end.checked_add(extra_len))
            .and_then(|end| end.checked_add(comment_len))
            .ok_or_else(|| {
                DataIntakeValidationError::new("The XLSX central directory is invalid")
            })?;
        if record_end > central_end {
            return Err(DataIntakeValidationError::new(
                "The XLSX central directory is truncated",
            ));
        }
        let name = &bytes[cursor + 46..cursor + 46 + name_len];
        if name
            .windows(b"vbaProject.bin".len())
            .any(|part| part.eq_ignore_ascii_case(b"vbaProject.bin"))
            || name
                .windows(b"externalLinks/".len())
                .any(|part| part.eq_ignore_ascii_case(b"externalLinks/"))
        {
            return Err(DataIntakeValidationError::new(
                "The XLSX file contains macros or external links",
            ));
        }
        expanded_bytes = expanded_bytes.checked_add(expanded).ok_or_else(|| {
            DataIntakeValidationError::new("The XLSX archive is too large")
        })?;
        if expanded_bytes > MAX_XLSX_EXPANDED_BYTES
            || (compressed == 0 && expanded > 0)
            || (compressed > 0 && expanded > compressed.saturating_mul(100))
        {
            return Err(DataIntakeValidationError::new(
                "The XLSX archive exceeds decompression safety limits",
            ));
        }
        cursor = record_end;
    }
    if cursor != central_end {
        return Err(DataIntakeValidationError::new(
            "The XLSX central directory is invalid",
        ));
    }
    Ok(())
}

pub fn validate_json_shape(
    value: &Value,
    depth: usize,
    count: &mut usize,
) -> Result<(), DataIntakeValidationError> {
    if depth > MAX_JSON_DEPTH {
        return Err(DataIntakeValidationError::new("JSON nesting is too deep"));
    }
    *count += 1;
    if *count > MAX_JSON_NODES {
        return Err(DataIntakeValidationError::new(
            "JSON contains too many values",
        ));
    }
    match value {
        Value::Object(map) => map
            .values()
            .try_for_each(|value| validate_json_shape(value, depth + 1, count)),
        Value::Array(values) => values
            .iter()
            .try_for_each(|value| validate_json_shape(value, depth + 1, count)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parameter_answers_are_exact_and_finite() {
        let parameter = DataRequirementParameter {
            name: "speed".to_string(),
            display_name: "平均速度".to_string(),
            description: String::new(),
            data_type: "number".to_string(),
            unit: Some("km/h".to_string()),
            required: true,
        };
        let answer = DataIntakeParameterAnswer {
            name: "speed".to_string(),
            value: json!(30),
            unit: Some("km/h".to_string()),
            source: "user".to_string(),
        };
        assert!(validate_parameter_answers(&[parameter], &[answer]).is_ok());
    }

    #[test]
    fn file_policy_rejects_paths_and_legacy_excel() {
        assert!(validate_file_name("data.csv").is_ok());
        assert!(validate_file_name("nested/data.csv").is_err());
        assert!(validate_file_name("data.xls").is_err());
    }
}
