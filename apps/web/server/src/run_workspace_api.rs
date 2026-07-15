use std::path::{Component, Path, PathBuf};
use std::process::Command;

use open_web_codex_platform_contracts::GitStatusFile;
use uuid::Uuid;

pub fn workspace_root(data_root: &Path, workspace_key: &str) -> Result<PathBuf, String> {
    for component in Path::new(workspace_key).components() {
        if matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            return Err("workspace key is invalid".to_string());
        }
    }
    let root = data_root.join("workspaces").join(workspace_key);
    if !root.exists() {
        return Err("workspace is not provisioned".to_string());
    }
    root.canonicalize()
        .map_err(|error| format!("workspace path is unavailable: {error}"))
}

pub fn resolve_workspace_file(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let trimmed = relative_path.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return Err("file path is required".to_string());
    }
    if trimmed.contains('\0') {
        return Err("file path is invalid".to_string());
    }
    let candidate = root.join(trimmed);
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("file is unavailable: {error}"))?;
    if !canonical.starts_with(root) {
        return Err("file path escapes workspace".to_string());
    }
    if !canonical.is_file() {
        return Err("path is not a file".to_string());
    }
    Ok(canonical)
}

pub fn list_workspace_files(root: &Path, limit: usize) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files, limit)?;
    files.sort();
    Ok(files)
}

fn collect_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<String>,
    limit: usize,
) -> Result<(), String> {
    if files.len() >= limit {
        return Ok(());
    }
    for entry in std::fs::read_dir(current).map_err(|error| format!("read dir failed: {error}"))? {
        let entry = entry.map_err(|error| format!("read dir entry failed: {error}"))?;
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some(".git") {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|error| format!("metadata failed: {error}"))?;
        if metadata.is_dir() {
            collect_files(root, &path, files, limit)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "path escape".to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            files.push(relative);
            if files.len() >= limit {
                return Ok(());
            }
        }
    }
    Ok(())
}

pub fn read_workspace_file_limited(path: &Path, max_bytes: usize) -> Result<(String, bool), String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read file failed: {error}"))?;
    let truncated = bytes.len() > max_bytes;
    let slice = if truncated { &bytes[..max_bytes] } else { &bytes };
    let content = String::from_utf8_lossy(slice).into_owned();
    Ok((content, truncated))
}

pub async fn load_run_workspace_key(
    db: &sqlx::PgPool,
    run_id: Uuid,
) -> Result<String, String> {
    let key = sqlx::query_scalar::<_, String>(
        "SELECT workspace_key FROM run_workspaces WHERE run_id = $1 AND state = 'ready'",
    )
    .bind(run_id)
    .fetch_optional(db)
    .await
    .map_err(|error| format!("workspace lookup failed: {error}"))?;
    key.ok_or_else(|| "run workspace is not ready".to_string())
}

pub fn git_status(root: &Path) -> Result<Vec<GitStatusFile>, String> {
    let output = Command::new("git")
        .args(["-C", root.to_string_lossy().as_ref(), "status", "--porcelain"])
        .output()
        .map_err(|error| format!("git status failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git status exited with status {}",
            output.status
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let files = stdout
        .lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }
            let status = line[..2].trim().to_string();
            let path = line[3..].trim().to_string();
            if path.is_empty() {
                return None;
            }
            Some(GitStatusFile { path, status })
        })
        .collect();
    Ok(files)
}

pub fn data_root_from_env() -> PathBuf {
    std::env::var("DATA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("open-web-codex-data"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_path_escape() {
        let root = std::env::temp_dir().join(format!("owc-path-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("safe.txt"), "ok").unwrap();
        let canonical = root.canonicalize().unwrap();
        let err = resolve_workspace_file(&canonical, "../etc/passwd");
        assert!(err.is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_git_status_lines() {
        let root = std::env::temp_dir().join(format!("owc-git-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("README.md"), "hello").unwrap();
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .unwrap();
        assert!(init.success());
        let status = git_status(&root.canonicalize().unwrap()).unwrap();
        assert!(status.iter().any(|entry| entry.path == "README.md"));
        let _ = fs::remove_dir_all(root);
    }
}
