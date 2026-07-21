mod validation;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::{OsStr, OsString};
use std::fs::OpenOptions;
use std::path::{Component, Path, PathBuf};
use std::process::{ExitStatus, Output};
use std::sync::{Arc, Mutex as StdMutex};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;
use tokio::sync::{Mutex, OwnedMutexGuard};
use uuid::Uuid;

pub use validation::{GitSourcePolicy, ValidatedGitRef, ValidatedGitSource};

const LARGE_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Error)]
pub enum GitRuntimeError {
    #[error("invalid Git source: {0}")]
    InvalidSource(String),
    #[error("invalid Git ref: {0}")]
    InvalidRef(String),
    #[error("unsafe workspace path: {0}")]
    UnsafePath(String),
    #[error("workspace conflict: {0}")]
    Conflict(String),
    #[error("Git {operation} failed: {message}")]
    Git {
        operation: &'static str,
        message: String,
    },
    #[error("I/O error during {operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("no selected changes are available to commit")]
    NoChanges,
}

#[derive(Debug, Clone)]
pub struct GitRuntimeConfig {
    pub root: PathBuf,
    pub git_bin: PathBuf,
    pub source_policy: GitSourcePolicy,
}

impl GitRuntimeConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            git_bin: PathBuf::from("git"),
            source_policy: GitSourcePolicy::remote_only(),
        }
    }

    pub fn with_local_sources(mut self) -> Self {
        self.source_policy = GitSourcePolicy::allow_local();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCheckout {
    pub workspace_id: Uuid,
    pub head_commit: String,
    pub branch: String,
    #[serde(skip)]
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub status: String,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub binary: bool,
    pub size_bytes: Option<u64>,
    pub large: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatus {
    pub branch: String,
    pub head_commit: String,
    pub changes: Vec<FileChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitAuthor {
    pub name: String,
    pub email: String,
}

#[derive(Clone)]
pub struct GitRuntime {
    config: GitRuntimeConfig,
    mirrors: PathBuf,
    workspaces: PathBuf,
    locks: PathBuf,
    process_locks: Arc<StdMutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl GitRuntime {
    pub fn new(config: GitRuntimeConfig) -> Result<Self, GitRuntimeError> {
        reject_symlink(&config.root, "runner root")?;
        create_private_dir(&config.root, "runner root")?;
        let root = canonicalize(&config.root, "runner root")?;
        let mirrors = create_owned_child(&root, "mirrors")?;
        let workspaces = create_owned_child(&root, "workspaces")?;
        let locks = create_owned_child(&root, "locks")?;
        Ok(Self {
            config,
            mirrors,
            workspaces,
            locks,
            process_locks: Arc::new(StdMutex::new(HashMap::new())),
        })
    }

    pub fn validate_source(&self, source: &str) -> Result<ValidatedGitSource, GitRuntimeError> {
        ValidatedGitSource::parse(source, self.config.source_policy)
    }

    pub fn validate_ref(&self, git_ref: &str) -> Result<ValidatedGitRef, GitRuntimeError> {
        ValidatedGitRef::parse(git_ref)
    }

    pub fn workspace_path(&self, workspace_id: Uuid) -> PathBuf {
        self.workspaces.join(workspace_id.to_string())
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspaces
    }

    pub async fn provision(
        &self,
        project_id: Uuid,
        workspace_id: Uuid,
        source: &ValidatedGitSource,
        git_ref: &ValidatedGitRef,
    ) -> Result<WorkspaceCheckout, GitRuntimeError> {
        let _lock = self.acquire_project_lock(project_id).await?;
        let mirror = self.ensure_mirror(project_id, workspace_id, source).await?;
        let commit = self.resolve_commit(&mirror, git_ref).await?;
        let workspace = self.workspace_path(workspace_id);
        reject_existing_workspace(&workspace)?;

        let result = async {
            self.git(
                "workspace clone",
                None,
                [
                    OsString::from("clone"),
                    OsString::from("--no-hardlinks"),
                    OsString::from("--reference-if-able"),
                    mirror.as_os_str().to_owned(),
                    OsString::from("--no-checkout"),
                    mirror.as_os_str().to_owned(),
                    workspace.as_os_str().to_owned(),
                ],
            )
            .await?;
            ensure_owned_workspace(&self.workspaces, &workspace)?;
            self.git(
                "workspace checkout",
                Some(&workspace),
                [
                    OsString::from("checkout"),
                    OsString::from("--detach"),
                    commit.clone().into(),
                ],
            )
            .await?;
            let branch = format!("codex-runs/{workspace_id}");
            self.git(
                "workspace branch",
                Some(&workspace),
                [
                    OsString::from("switch"),
                    OsString::from("-c"),
                    branch.clone().into(),
                ],
            )
            .await?;
            self.git(
                "workspace remote",
                Some(&workspace),
                [
                    OsString::from("remote"),
                    OsString::from("set-url"),
                    OsString::from("origin"),
                    OsString::from(source.as_str()),
                ],
            )
            .await?;
            Ok(WorkspaceCheckout {
                workspace_id,
                head_commit: commit,
                branch,
                root: workspace.clone(),
            })
        }
        .await;

        if result.is_err() {
            remove_internal_dir(&self.workspaces, &workspace, "failed workspace cleanup")?;
        }
        result
    }

    pub async fn status(&self, workspace_id: Uuid) -> Result<WorkspaceStatus, GitRuntimeError> {
        let _lock = self.acquire_workspace_lock(workspace_id).await;
        let workspace = self.require_workspace(workspace_id)?;
        let status = self
            .git_output(
                "workspace status",
                Some(&workspace),
                [
                    OsString::from("status"),
                    OsString::from("--porcelain=v1"),
                    OsString::from("-z"),
                    OsString::from("--untracked-files=all"),
                ],
                &[0],
            )
            .await?;
        let mut changes = parse_porcelain(&status.stdout)?;
        let numstat = self
            .git_output(
                "workspace diff",
                Some(&workspace),
                [
                    OsString::from("diff"),
                    OsString::from("--numstat"),
                    OsString::from("-z"),
                    OsString::from("HEAD"),
                    OsString::from("--"),
                ],
                &[0],
            )
            .await?;
        apply_numstat(&mut changes, &numstat.stdout)?;
        for change in &mut changes {
            change.size_bytes = safe_file_size(&workspace, &change.path)?;
            change.large = change
                .size_bytes
                .is_some_and(|size| size > LARGE_FILE_BYTES);
        }
        let branch = self
            .git_text(
                "workspace branch",
                Some(&workspace),
                ["branch", "--show-current"],
            )
            .await?;
        let head_commit = self
            .git_text("workspace head", Some(&workspace), ["rev-parse", "HEAD"])
            .await?;
        Ok(WorkspaceStatus {
            branch,
            head_commit,
            changes,
        })
    }

    pub async fn commit_selected(
        &self,
        workspace_id: Uuid,
        selected_paths: &[String],
        message: &str,
        author: &CommitAuthor,
    ) -> Result<String, GitRuntimeError> {
        validate_commit_input(selected_paths, message, author)?;
        let _lock = self.acquire_workspace_lock(workspace_id).await;
        let workspace = self.require_workspace(workspace_id)?;
        let selected = selected_paths.iter().cloned().collect::<BTreeSet<_>>();
        let available = self
            .git_output(
                "read workspace changes",
                Some(&workspace),
                [
                    OsString::from("status"),
                    OsString::from("--porcelain=v1"),
                    OsString::from("-z"),
                    OsString::from("--untracked-files=all"),
                ],
                &[0],
            )
            .await?;
        let available = parse_porcelain(&available.stdout)?
            .into_iter()
            .map(|change| change.path)
            .collect::<BTreeSet<_>>();
        if !selected.is_subset(&available) {
            return Err(GitRuntimeError::Conflict(
                "selection contains paths that are not exact workspace changes".to_string(),
            ));
        }
        let before = self.staged_paths(&workspace).await?;
        if !before.is_subset(&selected) {
            return Err(GitRuntimeError::Conflict(
                "workspace contains staged paths outside the selection".to_string(),
            ));
        }
        let mut add_args = vec![OsString::from("add"), OsString::from("--")];
        add_args.extend(selected_paths.iter().map(OsString::from));
        self.git("stage selected files", Some(&workspace), add_args)
            .await?;
        let staged = self.staged_paths(&workspace).await?;
        if staged.is_empty() {
            return Err(GitRuntimeError::NoChanges);
        }
        if !staged.is_subset(&selected) {
            return Err(GitRuntimeError::Conflict(
                "staged paths changed while preparing the commit".to_string(),
            ));
        }
        self.git(
            "commit selected files",
            Some(&workspace),
            [
                OsString::from("-c"),
                OsString::from(format!("user.name={}", author.name)),
                OsString::from("-c"),
                OsString::from(format!("user.email={}", author.email)),
                OsString::from("commit"),
                OsString::from("--no-gpg-sign"),
                OsString::from("-m"),
                OsString::from(message),
            ],
        )
        .await?;
        self.git_text("read commit", Some(&workspace), ["rev-parse", "HEAD"])
            .await
    }

    pub async fn remove_workspace(&self, workspace_id: Uuid) -> Result<(), GitRuntimeError> {
        let _lock = self.acquire_workspace_lock(workspace_id).await;
        let workspace = self.workspace_path(workspace_id);
        remove_internal_dir(&self.workspaces, &workspace, "workspace removal")
    }

    async fn ensure_mirror(
        &self,
        project_id: Uuid,
        workspace_id: Uuid,
        source: &ValidatedGitSource,
    ) -> Result<PathBuf, GitRuntimeError> {
        let mirror = self.mirrors.join(format!("{project_id}.git"));
        reject_symlink(&mirror, "repository mirror")?;
        if !mirror.exists() {
            let temporary = self
                .mirrors
                .join(format!(".{project_id}-{workspace_id}.tmp"));
            remove_internal_dir(&self.mirrors, &temporary, "stale mirror cleanup")?;
            let clone = self
                .git(
                    "mirror clone",
                    None,
                    [
                        OsString::from("clone"),
                        OsString::from("--mirror"),
                        OsString::from("--no-tags"),
                        OsString::from("--"),
                        OsString::from(source.as_str()),
                        temporary.as_os_str().to_owned(),
                    ],
                )
                .await;
            if let Err(error) = clone {
                remove_internal_dir(&self.mirrors, &temporary, "failed mirror cleanup")?;
                return Err(error);
            }
            std::fs::rename(&temporary, &mirror).map_err(|source| GitRuntimeError::Io {
                operation: "publish repository mirror",
                source,
            })?;
        }
        ensure_owned_workspace(&self.mirrors, &mirror)?;
        self.git(
            "mirror remote",
            None,
            [
                OsString::from("--git-dir"),
                mirror.as_os_str().to_owned(),
                OsString::from("remote"),
                OsString::from("set-url"),
                OsString::from("origin"),
                OsString::from(source.as_str()),
            ],
        )
        .await?;
        self.git(
            "mirror fetch",
            None,
            [
                OsString::from("--git-dir"),
                mirror.as_os_str().to_owned(),
                OsString::from("fetch"),
                OsString::from("--prune"),
                OsString::from("--no-tags"),
                OsString::from("origin"),
                OsString::from("+refs/heads/*:refs/heads/*"),
            ],
        )
        .await?;
        Ok(mirror)
    }

    async fn resolve_commit(
        &self,
        mirror: &Path,
        git_ref: &ValidatedGitRef,
    ) -> Result<String, GitRuntimeError> {
        self.git_text_os(
            "resolve Git ref",
            None,
            [
                OsString::from("--git-dir"),
                mirror.as_os_str().to_owned(),
                OsString::from("rev-parse"),
                OsString::from("--verify"),
                OsString::from(format!("refs/heads/{}^{{commit}}", git_ref.as_str())),
            ],
        )
        .await
    }

    fn require_workspace(&self, workspace_id: Uuid) -> Result<PathBuf, GitRuntimeError> {
        let workspace = self.workspace_path(workspace_id);
        ensure_owned_workspace(&self.workspaces, &workspace)?;
        if !workspace.join(".git").exists() {
            return Err(GitRuntimeError::Conflict(
                "workspace is not a Git checkout".to_string(),
            ));
        }
        Ok(workspace)
    }

    async fn staged_paths(&self, workspace: &Path) -> Result<BTreeSet<String>, GitRuntimeError> {
        let output = self
            .git_output(
                "read staged paths",
                Some(workspace),
                [
                    OsString::from("diff"),
                    OsString::from("--cached"),
                    OsString::from("--name-only"),
                    OsString::from("-z"),
                    OsString::from("--"),
                ],
                &[0],
            )
            .await?;
        nul_strings(&output.stdout).map(|paths| paths.into_iter().collect())
    }

    async fn git<I>(
        &self,
        operation: &'static str,
        cwd: Option<&Path>,
        args: I,
    ) -> Result<(), GitRuntimeError>
    where
        I: IntoIterator<Item = OsString>,
    {
        self.git_output(operation, cwd, args, &[0])
            .await
            .map(|_| ())
    }

    async fn git_text<const N: usize, S: AsRef<OsStr>>(
        &self,
        operation: &'static str,
        cwd: Option<&Path>,
        args: [S; N],
    ) -> Result<String, GitRuntimeError> {
        self.git_text_os(operation, cwd, args.map(|value| value.as_ref().to_owned()))
            .await
    }

    async fn git_text_os<I>(
        &self,
        operation: &'static str,
        cwd: Option<&Path>,
        args: I,
    ) -> Result<String, GitRuntimeError>
    where
        I: IntoIterator<Item = OsString>,
    {
        let output = self.git_output(operation, cwd, args, &[0]).await?;
        String::from_utf8(output.stdout)
            .map(|value| value.trim().to_string())
            .map_err(|_| GitRuntimeError::Git {
                operation,
                message: "Git returned non-UTF-8 output".to_string(),
            })
    }

    async fn git_output<I>(
        &self,
        operation: &'static str,
        cwd: Option<&Path>,
        args: I,
        accepted: &[i32],
    ) -> Result<Output, GitRuntimeError>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut command = Command::new(&self.config.git_bin);
        command
            .args(["-c", "core.hooksPath=/dev/null"])
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_GLOBAL", null_device())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("LC_ALL", "C");
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        let output = command
            .output()
            .await
            .map_err(|source| GitRuntimeError::Io { operation, source })?;
        let code = output.status.code().unwrap_or(-1);
        if accepted.contains(&code) {
            return Ok(output);
        }
        Err(git_failure(operation, &output.status, &output.stderr))
    }

    async fn acquire_project_lock(&self, project_id: Uuid) -> Result<RuntimeLock, GitRuntimeError> {
        self.acquire_lock(format!("project-{project_id}"), true)
            .await
    }

    async fn acquire_workspace_lock(&self, workspace_id: Uuid) -> RuntimeLock {
        self.acquire_lock(format!("workspace-{workspace_id}"), false)
            .await
            .expect("workspace lock path is server-generated")
    }

    async fn acquire_lock(
        &self,
        name: String,
        file_lock: bool,
    ) -> Result<RuntimeLock, GitRuntimeError> {
        let lock = {
            let mut locks = self.process_locks.lock().expect("Git lock map poisoned");
            locks
                .entry(name.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let guard = lock.lock_owned().await;
        let file = if file_lock {
            Some(acquire_file_lock(self.locks.join(format!("{name}.lock"))).await?)
        } else {
            None
        };
        Ok(RuntimeLock {
            _guard: guard,
            _file: file,
        })
    }
}

struct RuntimeLock {
    _guard: OwnedMutexGuard<()>,
    _file: Option<std::fs::File>,
}

fn git_failure(operation: &'static str, status: &ExitStatus, stderr: &[u8]) -> GitRuntimeError {
    let message = String::from_utf8_lossy(stderr);
    let message = message.lines().take(4).collect::<Vec<_>>().join(" ");
    GitRuntimeError::Git {
        operation,
        message: if message.is_empty() {
            format!("process exited with {}", status.code().unwrap_or(-1))
        } else {
            message
        },
    }
}

fn parse_porcelain(bytes: &[u8]) -> Result<Vec<FileChange>, GitRuntimeError> {
    let records = bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .collect::<Vec<_>>();
    let mut changes = BTreeMap::new();
    let mut index = 0;
    while index < records.len() {
        let record = std::str::from_utf8(records[index]).map_err(|_| GitRuntimeError::Git {
            operation: "workspace status",
            message: "Git returned a non-UTF-8 path".to_string(),
        })?;
        if record.len() < 4 || record.as_bytes()[2] != b' ' {
            return Err(GitRuntimeError::Git {
                operation: "workspace status",
                message: "Git returned an invalid porcelain record".to_string(),
            });
        }
        let status = record[..2].to_string();
        let path = record[3..].to_string();
        validate_relative_path(&path)?;
        changes.insert(
            path.clone(),
            FileChange {
                path,
                status: status.clone(),
                additions: None,
                deletions: None,
                binary: false,
                size_bytes: None,
                large: false,
            },
        );
        if status.contains('R') || status.contains('C') {
            index += 1;
        }
        index += 1;
    }
    Ok(changes.into_values().collect())
}

fn apply_numstat(changes: &mut [FileChange], bytes: &[u8]) -> Result<(), GitRuntimeError> {
    let mut by_path = changes
        .iter_mut()
        .map(|change| (change.path.clone(), change))
        .collect::<BTreeMap<_, _>>();
    for record in bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let record = std::str::from_utf8(record).map_err(|_| GitRuntimeError::Git {
            operation: "workspace diff",
            message: "Git returned non-UTF-8 diff metadata".to_string(),
        })?;
        let mut fields = record.splitn(3, '\t');
        let additions = fields.next().unwrap_or_default();
        let deletions = fields.next().unwrap_or_default();
        let path = fields.next().unwrap_or_default();
        if let Some(change) = by_path.get_mut(path) {
            change.binary = additions == "-" || deletions == "-";
            change.additions = additions.parse().ok();
            change.deletions = deletions.parse().ok();
        }
    }
    Ok(())
}

fn nul_strings(bytes: &[u8]) -> Result<Vec<String>, GitRuntimeError> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| {
            let value = std::str::from_utf8(record).map_err(|_| GitRuntimeError::Git {
                operation: "read Git paths",
                message: "Git returned a non-UTF-8 path".to_string(),
            })?;
            validate_relative_path(value)?;
            Ok(value.to_string())
        })
        .collect()
}

fn validate_commit_input(
    selected_paths: &[String],
    message: &str,
    author: &CommitAuthor,
) -> Result<(), GitRuntimeError> {
    if selected_paths.is_empty() {
        return Err(GitRuntimeError::NoChanges);
    }
    for path in selected_paths {
        validate_relative_path(path)?;
    }
    if message.trim().is_empty() || message.len() > 10_000 || message.contains('\0') {
        return Err(GitRuntimeError::Conflict(
            "invalid commit message".to_string(),
        ));
    }
    if author.name.trim().is_empty()
        || author.name.contains(['\0', '\n', '\r'])
        || author.email.contains(['\0', '\n', '\r'])
        || !author.email.contains('@')
    {
        return Err(GitRuntimeError::Conflict(
            "invalid commit author".to_string(),
        ));
    }
    Ok(())
}

fn validate_relative_path(value: &str) -> Result<(), GitRuntimeError> {
    if value.is_empty() || value.contains('\0') {
        return Err(GitRuntimeError::UnsafePath("empty or NUL path".to_string()));
    }
    let path = Path::new(value);
    if value.starts_with("./")
        || value.contains("//")
        || path.components().any(|component| {
            matches!(
                component,
                Component::CurDir
                    | Component::ParentDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
    {
        return Err(GitRuntimeError::UnsafePath(
            "path escapes the workspace".to_string(),
        ));
    }
    Ok(())
}

fn safe_file_size(workspace: &Path, relative: &str) -> Result<Option<u64>, GitRuntimeError> {
    validate_relative_path(relative)?;
    let path = workspace.join(relative);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(GitRuntimeError::Io {
                operation: "inspect workspace file",
                source,
            })
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(None);
    }
    let canonical = canonicalize(&path, "workspace file")?;
    if !canonical.starts_with(workspace) {
        return Err(GitRuntimeError::UnsafePath(
            "workspace file escaped through a symlink".to_string(),
        ));
    }
    Ok(Some(metadata.len()))
}

fn reject_existing_workspace(path: &Path) -> Result<(), GitRuntimeError> {
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(GitRuntimeError::Conflict(
            "workspace path already exists".to_string(),
        ));
    }
    Ok(())
}

fn ensure_owned_workspace(parent: &Path, path: &Path) -> Result<(), GitRuntimeError> {
    reject_symlink(path, "workspace")?;
    let canonical = canonicalize(path, "workspace")?;
    if canonical.parent() != Some(parent) {
        return Err(GitRuntimeError::UnsafePath(
            "workspace is outside the runner root".to_string(),
        ));
    }
    Ok(())
}

fn remove_internal_dir(
    parent: &Path,
    path: &Path,
    operation: &'static str,
) -> Result<(), GitRuntimeError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(GitRuntimeError::Io { operation, source }),
    };
    if metadata.file_type().is_symlink() || path.parent() != Some(parent) {
        return Err(GitRuntimeError::UnsafePath(format!(
            "refused {operation} outside owned root"
        )));
    }
    std::fs::remove_dir_all(path).map_err(|source| GitRuntimeError::Io { operation, source })
}

fn create_owned_child(root: &Path, name: &str) -> Result<PathBuf, GitRuntimeError> {
    let path = root.join(name);
    reject_symlink(&path, "runner directory")?;
    create_private_dir(&path, "runner directory")?;
    let canonical = canonicalize(&path, "runner directory")?;
    if canonical.parent() != Some(root) {
        return Err(GitRuntimeError::UnsafePath(format!(
            "runner {name} directory escaped its root"
        )));
    }
    Ok(canonical)
}

fn create_private_dir(path: &Path, operation: &'static str) -> Result<(), GitRuntimeError> {
    std::fs::create_dir_all(path).map_err(|source| GitRuntimeError::Io { operation, source })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|source| GitRuntimeError::Io { operation, source })?;
    }
    Ok(())
}

fn reject_symlink(path: &Path, operation: &'static str) -> Result<(), GitRuntimeError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(GitRuntimeError::UnsafePath(
            format!("{operation} must not be a symlink"),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(GitRuntimeError::Io { operation, source }),
    }
}

fn canonicalize(path: &Path, operation: &'static str) -> Result<PathBuf, GitRuntimeError> {
    path.canonicalize()
        .map_err(|source| GitRuntimeError::Io { operation, source })
}

async fn acquire_file_lock(path: PathBuf) -> Result<std::fs::File, GitRuntimeError> {
    tokio::task::spawn_blocking(move || {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|source| GitRuntimeError::Io {
                operation: "open mirror lock",
                source,
            })?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if result != 0 {
                return Err(GitRuntimeError::Io {
                    operation: "lock repository mirror",
                    source: std::io::Error::last_os_error(),
                });
            }
        }
        Ok(file)
    })
    .await
    .map_err(|error| GitRuntimeError::Conflict(format!("mirror lock task failed: {error}")))?
}

#[cfg(windows)]
fn null_device() -> &'static str {
    "NUL"
}

#[cfg(not(windows))]
fn null_device() -> &'static str {
    "/dev/null"
}
