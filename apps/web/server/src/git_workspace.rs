use std::path::{Component, Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

/// Validates a Git remote URL for obvious injection patterns.
pub fn validate_git_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("git url is required".to_string());
    }
    if trimmed.contains('\0') || trimmed.contains("..") {
        return Err("git url contains invalid characters".to_string());
    }
    let allowed = trimmed.starts_with("https://")
        || trimmed.starts_with("http://")
        || trimmed.starts_with("git@")
        || trimmed.starts_with("ssh://")
        || trimmed.starts_with("file://");
    if !allowed {
        return Err("git url must use https, http, ssh, git@, or file".to_string());
    }
    Ok(())
}

pub fn validate_branch_name(branch: &str) -> Result<(), String> {
    let trimmed = branch.trim();
    if trimmed.is_empty() {
        return Err("branch is required".to_string());
    }
    if trimmed.contains("..") || trimmed.starts_with('-') || trimmed.contains(char::is_whitespace) {
        return Err("branch name is invalid".to_string());
    }
    Ok(())
}

fn assert_within_root(root: &Path, candidate: &Path) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("workspace root is unavailable: {error}"))?;
    let candidate = candidate
        .canonicalize()
        .map_err(|error| format!("workspace path is unavailable: {error}"))?;
    if !candidate.starts_with(&root) {
        return Err("workspace path escapes the data root".to_string());
    }
    Ok(candidate)
}

/// Provisions an isolated workspace directory for a run.
///
/// Returns a stable workspace key safe to store in PostgreSQL.
pub fn provision_run_workspace(
    data_root: &Path,
    run_id: Uuid,
    git_url: &str,
    branch: &str,
) -> Result<String, String> {
    validate_git_url(git_url)?;
    validate_branch_name(branch)?;

    let workspaces_root = data_root.join("workspaces");
    std::fs::create_dir_all(&workspaces_root)
        .map_err(|error| format!("failed to create workspaces root: {error}"))?;

    let workspace_key = format!("runs/{run_id}");
    let destination = workspaces_root.join(&workspace_key);
    if destination.exists() {
        return Err("workspace already exists for this run".to_string());
    }

    for component in Path::new(&workspace_key).components() {
        if matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            return Err("workspace key is invalid".to_string());
        }
    }

    std::fs::create_dir_all(destination.parent().unwrap())
        .map_err(|error| format!("failed to create workspace parent: {error}"))?;

    let status = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--branch",
            branch,
            git_url,
            &destination.to_string_lossy(),
        ])
        .status()
        .map_err(|error| format!("failed to spawn git: {error}"))?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&destination);
        return Err(format!("git clone failed with status {status}"));
    }

    assert_within_root(&workspaces_root, &destination)?;
    Ok(workspace_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_git_urls() {
        assert!(validate_git_url("javascript:alert(1)").is_err());
        assert!(validate_git_url("https://github.com/example/repo.git").is_ok());
    }
}
