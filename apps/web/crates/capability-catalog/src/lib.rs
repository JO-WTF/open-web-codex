//! General catalog validation and Release compilation.
//!
//! This crate deliberately has no database, filesystem or Runtime dependency.
//! Those lifecycles belong to the Platform Server and Profile Host. Keeping the
//! compiler pure makes Draft validation and Release creation deterministic in
//! Web, seed code and tests.

use chrono::{DateTime, Utc};
use open_web_codex_platform_contracts::{
    CapabilityReleaseSummary, CatalogDependency, CatalogDraftContent, CatalogPackageFile,
    CatalogResourceKind, ReleaseIdentity,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

const MAX_FILES: usize = 64;
const MAX_FILE_BYTES: usize = 512 * 1024;
const MAX_TOTAL_FILE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CatalogCompileError {
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct CompiledCatalogRelease {
    pub content: CatalogDraftContent,
    pub content_sha256: String,
    pub execution_semantics_sha256: String,
}

/// Validate and normalize a Draft without assigning a Release version.
pub fn compile_draft(
    kind: CatalogResourceKind,
    resource_id: &str,
    content: CatalogDraftContent,
) -> Result<CompiledCatalogRelease, CatalogCompileError> {
    validate_resource_id(resource_id, "resource id")?;
    if content.files.len() > MAX_FILES {
        return Err(CatalogCompileError::Invalid(
            "a capability package may contain at most 64 files".to_string(),
        ));
    }
    let mut files = content.files;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    if files
        .windows(2)
        .any(|pair| pair[0].path == pair[1].path)
    {
        return Err(CatalogCompileError::Invalid(
            "package file paths must be unique".to_string(),
        ));
    }
    let mut total_bytes = 0usize;
    for file in &files {
        validate_file(file)?;
        total_bytes = total_bytes
            .checked_add(file.content.len())
            .ok_or_else(|| CatalogCompileError::Invalid("package content is too large".into()))?;
    }
    if total_bytes > MAX_TOTAL_FILE_BYTES {
        return Err(CatalogCompileError::Invalid(
            "capability package content exceeds 4 MiB".to_string(),
        ));
    }
    match kind {
        CatalogResourceKind::ToolPackage => {
            require_file(&files, ".codex-plugin/plugin.json")?;
            require_file(&files, ".mcp.json")?;
        }
        CatalogResourceKind::SkillPackage => {
            let skill = files
                .iter()
                .find(|file| file.path.ends_with("SKILL.md"))
                .ok_or_else(|| {
                    CatalogCompileError::Invalid("Skill package must contain SKILL.md".into())
                })?;
            if !contains_chinese(&skill.content) {
                return Err(CatalogCompileError::Invalid(
                    "SKILL.md must contain Chinese instructions".to_string(),
                ));
            }
        }
        CatalogResourceKind::AgentDefinition
        | CatalogResourceKind::SupervisorDefinition
        | CatalogResourceKind::CopilotPackage => {
            if !content.definition.is_object() {
                return Err(CatalogCompileError::Invalid(
                    "definition must be a JSON object".to_string(),
                ));
            }
        }
    }
    let dependencies = normalize_dependencies(content.dependencies)?;
    let normalized = CatalogDraftContent {
        files,
        definition: content.definition,
        dependencies,
    };
    let serialized = serde_json::to_vec(&normalized)
        .map_err(|_| CatalogCompileError::Invalid("catalog content cannot be serialized".into()))?;
    let content_sha256 = digest(&serialized);
    let semantics = serde_json::to_vec(&json!({
        "kind": kind,
        "resourceId": resource_id,
        "definition": normalized.definition,
        "dependencies": normalized.dependencies,
        "filePaths": normalized.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(),
    }))
    .map_err(|_| CatalogCompileError::Invalid("execution semantics cannot be serialized".into()))?;
    Ok(CompiledCatalogRelease {
        content: normalized,
        content_sha256,
        execution_semantics_sha256: digest(&semantics),
    })
}

pub fn next_patch_version(existing: &[String]) -> String {
    let highest = existing
        .iter()
        .filter_map(|version| {
            let mut parts = version.split('.');
            let major = parts.next()?.parse::<u64>().ok()?;
            let minor = parts.next()?.parse::<u64>().ok()?;
            let patch = parts.next()?.parse::<u64>().ok()?;
            (parts.next().is_none()).then_some((major, minor, patch))
        })
        .max()
        .unwrap_or((1, 0, 0));
    format!("{}.{}.{}", highest.0, highest.1, highest.2 + 1)
}

pub fn release_summary(
    id: Uuid,
    kind: CatalogResourceKind,
    resource_id: String,
    release_version: String,
    display_name: String,
    description: String,
    content_sha256: String,
    execution_semantics_sha256: String,
    published_at: DateTime<Utc>,
) -> CapabilityReleaseSummary {
    CapabilityReleaseSummary {
        id,
        kind,
        resource_id: resource_id.clone(),
        release_version: release_version.clone(),
        display_name,
        description,
        identity: ReleaseIdentity {
            id,
            kind,
            resource_id,
            release_version,
            content_sha256,
            execution_semantics_sha256,
            published_at,
        },
    }
}

fn normalize_dependencies(
    dependencies: Vec<CatalogDependency>,
) -> Result<Vec<CatalogDependency>, CatalogCompileError> {
    let mut dependencies = dependencies;
    dependencies.sort_by(|left, right| {
        kind_name(left.kind)
            .cmp(kind_name(right.kind))
            .then(left.resource_id.cmp(&right.resource_id))
    });
    for pair in dependencies.windows(2) {
        if pair[0].kind == pair[1].kind && pair[0].resource_id == pair[1].resource_id {
            return Err(CatalogCompileError::Invalid(
                "catalog dependencies must be unique".to_string(),
            ));
        }
    }
    for dependency in &dependencies {
        validate_resource_id(&dependency.resource_id, "dependency resource id")?;
        if dependency.release_id.is_none()
            && dependency
                .release_version
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return Err(CatalogCompileError::Invalid(
                "every dependency must select an exact Release".to_string(),
            ));
        }
    }
    Ok(dependencies)
}

fn validate_resource_id(value: &str, label: &str) -> Result<(), CatalogCompileError> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
        || !value.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(CatalogCompileError::Invalid(format!(
            "{label} must use lowercase letters, digits, '.', '_' or '-'"
        )));
    }
    Ok(())
}

fn validate_file(file: &CatalogPackageFile) -> Result<(), CatalogCompileError> {
    if file.path.is_empty()
        || file.path.len() > 256
        || file.path.starts_with('/')
        || file
            .path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || file.path.contains('\\')
    {
        return Err(CatalogCompileError::Invalid(format!(
            "package path '{}' is unsafe",
            file.path
        )));
    }
    if file.content.len() > MAX_FILE_BYTES {
        return Err(CatalogCompileError::Invalid(format!(
            "package file '{}' exceeds 512 KiB",
            file.path
        )));
    }
    Ok(())
}

fn require_file(files: &[CatalogPackageFile], path: &str) -> Result<(), CatalogCompileError> {
    if files.iter().filter(|file| file.path == path).count() != 1 {
        return Err(CatalogCompileError::Invalid(format!(
            "package must contain exactly one {path}"
        )));
    }
    Ok(())
}

fn contains_chinese(value: &str) -> bool {
    value
        .chars()
        .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character))
}

fn kind_name(kind: CatalogResourceKind) -> &'static str {
    match kind {
        CatalogResourceKind::ToolPackage => "tool_package",
        CatalogResourceKind::SkillPackage => "skill_package",
        CatalogResourceKind::AgentDefinition => "agent_definition",
        CatalogResourceKind::SupervisorDefinition => "supervisor_definition",
        CatalogResourceKind::CopilotPackage => "copilot_package",
    }
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill_content() -> CatalogDraftContent {
        CatalogDraftContent {
            files: vec![CatalogPackageFile {
                path: "skills/demo/SKILL.md".to_string(),
                content: "## 适用问题\n用中文执行数据检查。".to_string(),
            }],
            definition: json!({"owner":"data"}),
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn normalizes_and_hashes_skill_draft() {
        let compiled = compile_draft(
            CatalogResourceKind::SkillPackage,
            "data-quality",
            skill_content(),
        )
        .unwrap();
        assert_eq!(compiled.content.files[0].path, "skills/demo/SKILL.md");
        assert_eq!(compiled.content_sha256.len(), 64);
        assert_eq!(compiled.execution_semantics_sha256.len(), 64);
    }

    #[test]
    fn rejects_non_chinese_skill() {
        let content = CatalogDraftContent {
            files: vec![CatalogPackageFile {
                path: "SKILL.md".to_string(),
                content: "Only English".to_string(),
            }],
            definition: json!({}),
            dependencies: Vec::new(),
        };
        assert!(compile_draft(CatalogResourceKind::SkillPackage, "demo", content).is_err());
    }

    #[test]
    fn rejects_duplicate_package_paths_before_release() {
        let mut content = skill_content();
        content.files.push(CatalogPackageFile {
            path: "skills/demo/SKILL.md".to_string(),
            content: "重复路径".to_string(),
        });
        let error = compile_draft(CatalogResourceKind::SkillPackage, "demo", content)
            .expect_err("duplicate paths must not be silently overwritten");
        assert!(error.to_string().contains("file paths must be unique"));
    }

    #[test]
    fn allocates_server_patch_version() {
        assert_eq!(next_patch_version(&[]), "1.0.1");
        assert_eq!(
            next_patch_version(&["1.0.0".to_string(), "1.0.4".to_string()]),
            "1.0.5"
        );
    }
}
