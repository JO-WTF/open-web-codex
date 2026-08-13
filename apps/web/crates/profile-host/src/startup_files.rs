use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;
use uuid::Uuid;

use crate::{create_private_directory, restrict_directory_permissions};

const MAX_STARTUP_FILE_BYTES: usize = 128 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 96;

/// One server-provided native Profile file that Codex discovers from its
/// standard roots. Ordinary files seed missing destinations; explicitly
/// managed package files keep their reserved destinations aligned at startup.
///
/// Destinations are intentionally limited to native Skill and Agent Role
/// locations. This is not a general Profile filesystem API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileStartupFile {
    destination: ProfileStartupFileDestination,
    ownership: ProfileStartupFileOwnership,
    contents: Vec<u8>,
}

/// One exact managed package destination to remove during Profile
/// reconciliation. Construction remains limited to native Skill and Agent
/// Role roots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileStartupFileRemoval {
    destination: ProfileStartupFileDestination,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileStartupFileOwnership {
    Seed,
    Managed,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ProfileStartupFileDestination {
    Skill { id: String },
    AgentRole { name: String },
}

impl ProfileStartupFile {
    pub fn skill(
        id: impl Into<String>,
        contents: impl Into<Vec<u8>>,
    ) -> Result<Self, ProfileStartupFileError> {
        let id = id.into();
        validate_identifier(&id, "Skill id")?;
        Self::new(
            ProfileStartupFileDestination::Skill { id },
            ProfileStartupFileOwnership::Seed,
            contents.into(),
        )
    }

    /// One Skill selected from an explicitly configured Copilot package. Its
    /// reserved destination is refreshed before the Profile process starts
    /// while unrelated Profile files remain user-owned.
    pub fn package_skill(
        id: impl Into<String>,
        contents: impl Into<Vec<u8>>,
    ) -> Result<Self, ProfileStartupFileError> {
        let id = id.into();
        validate_identifier(&id, "Skill id")?;
        Self::new(
            ProfileStartupFileDestination::Skill { id },
            ProfileStartupFileOwnership::Managed,
            contents.into(),
        )
    }

    pub fn agent_role(
        name: impl Into<String>,
        contents: impl Into<Vec<u8>>,
    ) -> Result<Self, ProfileStartupFileError> {
        let name = name.into();
        validate_identifier(&name, "Agent Role name")?;
        Self::new(
            ProfileStartupFileDestination::AgentRole { name },
            ProfileStartupFileOwnership::Seed,
            contents.into(),
        )
    }

    /// One Agent Role selected from an explicitly configured Copilot package.
    /// The package definition is the startup source of truth for this reserved
    /// Role name.
    pub fn package_agent_role(
        name: impl Into<String>,
        contents: impl Into<Vec<u8>>,
    ) -> Result<Self, ProfileStartupFileError> {
        let name = name.into();
        validate_identifier(&name, "Agent Role name")?;
        Self::new(
            ProfileStartupFileDestination::AgentRole { name },
            ProfileStartupFileOwnership::Managed,
            contents.into(),
        )
    }

    fn new(
        destination: ProfileStartupFileDestination,
        ownership: ProfileStartupFileOwnership,
        contents: Vec<u8>,
    ) -> Result<Self, ProfileStartupFileError> {
        if contents.is_empty() {
            return Err(ProfileStartupFileError::InvalidInput(
                "Profile startup file must not be empty".to_string(),
            ));
        }
        if contents.len() > MAX_STARTUP_FILE_BYTES {
            return Err(ProfileStartupFileError::InvalidInput(format!(
                "Profile startup file exceeds {MAX_STARTUP_FILE_BYTES} bytes"
            )));
        }
        Ok(Self {
            destination,
            ownership,
            contents,
        })
    }

    fn relative_path(&self) -> PathBuf {
        match &self.destination {
            ProfileStartupFileDestination::Skill { id } => {
                PathBuf::from("skills").join(id).join("SKILL.md")
            }
            ProfileStartupFileDestination::AgentRole { name } => {
                PathBuf::from("agents").join(format!("{name}.toml"))
            }
        }
    }
}

impl ProfileStartupFileRemoval {
    pub fn package_skill(id: impl Into<String>) -> Result<Self, ProfileStartupFileError> {
        let id = id.into();
        validate_identifier(&id, "Skill id")?;
        Ok(Self {
            destination: ProfileStartupFileDestination::Skill { id },
        })
    }

    pub fn package_agent_role(name: impl Into<String>) -> Result<Self, ProfileStartupFileError> {
        let name = name.into();
        validate_identifier(&name, "Agent Role name")?;
        Ok(Self {
            destination: ProfileStartupFileDestination::AgentRole { name },
        })
    }

    fn relative_path(&self) -> PathBuf {
        match &self.destination {
            ProfileStartupFileDestination::Skill { id } => {
                PathBuf::from("skills").join(id).join("SKILL.md")
            }
            ProfileStartupFileDestination::AgentRole { name } => {
                PathBuf::from("agents").join(format!("{name}.toml"))
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ProfileStartupFileError {
    #[error("invalid Profile startup file: {0}")]
    InvalidInput(String),
    #[error("unsafe Profile startup destination '{relative_path}': {message}")]
    UnsafePath {
        relative_path: String,
        message: String,
    },
    #[error("failed to materialize Profile startup file '{relative_path}': {source}")]
    Io {
        relative_path: String,
        #[source]
        source: io::Error,
    },
}

/// Reconcile one validated batch before an app-server starts. Every write is
/// staged and every destination is preflighted before publication. A publish
/// failure rolls back changes already made by this process.
pub fn reconcile_profile_startup_files(
    home: &Path,
    files: &[ProfileStartupFile],
    removed: &[ProfileStartupFileRemoval],
) -> Result<(), ProfileStartupFileError> {
    let mut destinations = HashSet::new();
    for file in files {
        let relative_path = file.relative_path();
        if !destinations.insert(relative_path.clone()) {
            return Err(ProfileStartupFileError::InvalidInput(format!(
                "duplicate Profile startup destination '{}'",
                relative_path.display()
            )));
        }
    }
    for removal in removed {
        let relative_path = removal.relative_path();
        if !destinations.insert(relative_path.clone()) {
            return Err(ProfileStartupFileError::InvalidInput(format!(
                "duplicate Profile startup destination '{}'",
                relative_path.display()
            )));
        }
    }
    let home = home
        .canonicalize()
        .map_err(|source| ProfileStartupFileError::Io {
            relative_path: ".".to_string(),
            source,
        })?;
    let mut changes = Vec::new();
    for file in files {
        match prepare_write(&home, file) {
            Ok(Some(change)) => changes.push(change),
            Ok(None) => {}
            Err(error) => {
                cleanup_staged(&changes);
                return Err(error);
            }
        }
    }
    for removal in removed {
        match prepare_removal(&home, &removal.relative_path()) {
            Ok(Some(change)) => changes.push(change),
            Ok(None) => {}
            Err(error) => {
                cleanup_staged(&changes);
                return Err(error);
            }
        }
    }
    publish_changes(changes)
}

struct PreparedChange {
    label: String,
    target: PathBuf,
    staged: Option<PathBuf>,
    replaces_existing: bool,
}

enum PublishedChange {
    New { target: PathBuf },
    Replaced { target: PathBuf, backup: PathBuf },
    Removed { target: PathBuf, backup: PathBuf },
}

fn prepare_write(
    home: &Path,
    file: &ProfileStartupFile,
) -> Result<Option<PreparedChange>, ProfileStartupFileError> {
    let relative_path = file.relative_path();
    let (label, parent, target) = checked_target(home, &relative_path)?;
    let replaces_existing = match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(ProfileStartupFileError::UnsafePath {
                relative_path: label,
                message: "target must be a regular file".to_string(),
            });
        }
        Ok(_) if file.ownership == ProfileStartupFileOwnership::Seed => return Ok(None),
        Ok(_) => {
            let current = fs::read(&target).map_err(|source| ProfileStartupFileError::Io {
                relative_path: label.clone(),
                source,
            })?;
            if current == file.contents {
                return Ok(None);
            }
            true
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(source) => {
            return Err(ProfileStartupFileError::Io {
                relative_path: label,
                source,
            });
        }
    };

    let file_name = target
        .file_name()
        .expect("checked startup target has a filename");
    let staged = parent.join(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        Uuid::now_v7()
    ));
    let write_result = (|| -> io::Result<()> {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)?;
        restrict_file_permissions(&output)?;
        output.write_all(&file.contents)?;
        output.sync_all()
    })();
    if let Err(source) = write_result {
        let _ = fs::remove_file(&staged);
        return Err(ProfileStartupFileError::Io {
            relative_path: label,
            source,
        });
    }
    Ok(Some(PreparedChange {
        label,
        target,
        staged: Some(staged),
        replaces_existing,
    }))
}

fn prepare_removal(
    home: &Path,
    relative_path: &Path,
) -> Result<Option<PreparedChange>, ProfileStartupFileError> {
    let (label, _parent, target) = checked_target(home, relative_path)?;
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(ProfileStartupFileError::UnsafePath {
                relative_path: label,
                message: "target must be a regular file".to_string(),
            })
        }
        Ok(_) => Ok(Some(PreparedChange {
            label,
            target,
            staged: None,
            replaces_existing: true,
        })),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ProfileStartupFileError::Io {
            relative_path: label,
            source,
        }),
    }
}

fn checked_target(
    home: &Path,
    relative_path: &Path,
) -> Result<(String, PathBuf, PathBuf), ProfileStartupFileError> {
    let label = relative_path_label(relative_path);
    let parent_relative = relative_path.parent().ok_or_else(|| {
        ProfileStartupFileError::InvalidInput("Profile startup destination has no parent".into())
    })?;
    let parent = ensure_private_relative_directory(home, parent_relative, &label)?;
    let file_name = relative_path.file_name().ok_or_else(|| {
        ProfileStartupFileError::InvalidInput("Profile startup destination has no filename".into())
    })?;
    let target = parent.join(file_name);
    Ok((label, parent, target))
}

fn publish_changes(changes: Vec<PreparedChange>) -> Result<(), ProfileStartupFileError> {
    let mut published = Vec::new();
    for change in &changes {
        let result = publish_change(change, &mut published);
        if let Err(source) = result {
            rollback_changes(&published);
            cleanup_staged(&changes);
            return Err(ProfileStartupFileError::Io {
                relative_path: change.label.clone(),
                source,
            });
        }
    }
    cleanup_staged(&changes);
    for change in published {
        let backup = match change {
            PublishedChange::Replaced { backup, .. } | PublishedChange::Removed { backup, .. } => {
                Some(backup)
            }
            PublishedChange::New { .. } => None,
        };
        if let Some(backup) = backup {
            if let Err(error) = fs::remove_file(&backup) {
                tracing::warn!(path = %backup.display(), error = %error, "failed to remove Profile startup rollback file");
            }
        }
    }
    Ok(())
}

fn publish_change(change: &PreparedChange, published: &mut Vec<PublishedChange>) -> io::Result<()> {
    if change.replaces_existing {
        let file_name = change
            .target
            .file_name()
            .expect("checked startup target has a filename");
        let backup = change.target.with_file_name(format!(
            ".{}.{}.rollback",
            file_name.to_string_lossy(),
            Uuid::now_v7()
        ));
        fs::rename(&change.target, &backup)?;
        match &change.staged {
            Some(staged) => {
                if let Err(error) = fs::rename(staged, &change.target) {
                    let _ = fs::rename(&backup, &change.target);
                    return Err(error);
                }
                published.push(PublishedChange::Replaced {
                    target: change.target.clone(),
                    backup,
                });
            }
            None => published.push(PublishedChange::Removed {
                target: change.target.clone(),
                backup,
            }),
        }
    } else {
        let staged = change
            .staged
            .as_ref()
            .expect("new startup file has staged contents");
        fs::hard_link(staged, &change.target)?;
        published.push(PublishedChange::New {
            target: change.target.clone(),
        });
    }
    Ok(())
}

fn rollback_changes(changes: &[PublishedChange]) {
    for change in changes.iter().rev() {
        match change {
            PublishedChange::New { target } => {
                let _ = fs::remove_file(target);
            }
            PublishedChange::Replaced { target, backup } => {
                let _ = fs::remove_file(target);
                let _ = fs::rename(backup, target);
            }
            PublishedChange::Removed { target, backup } => {
                let _ = fs::rename(backup, target);
            }
        }
    }
}

fn cleanup_staged(changes: &[PreparedChange]) {
    for staged in changes.iter().filter_map(|change| change.staged.as_ref()) {
        let _ = fs::remove_file(staged);
    }
}

fn ensure_private_relative_directory(
    home: &Path,
    relative: &Path,
    label: &str,
) -> Result<PathBuf, ProfileStartupFileError> {
    let mut current = home.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(ProfileStartupFileError::InvalidInput(
                "Profile startup destination must be relative".to_string(),
            ));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ProfileStartupFileError::UnsafePath {
                    relative_path: label.to_string(),
                    message: "parent directory must not be a symlink".to_string(),
                });
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(ProfileStartupFileError::UnsafePath {
                    relative_path: label.to_string(),
                    message: "parent must be a directory".to_string(),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                create_private_directory(&current).map_err(|source| {
                    ProfileStartupFileError::Io {
                        relative_path: label.to_string(),
                        source,
                    }
                })?;
            }
            Err(source) => {
                return Err(ProfileStartupFileError::Io {
                    relative_path: label.to_string(),
                    source,
                });
            }
        }
        restrict_directory_permissions(&current).map_err(|source| ProfileStartupFileError::Io {
            relative_path: label.to_string(),
            source,
        })?;
    }
    let canonical = current
        .canonicalize()
        .map_err(|source| ProfileStartupFileError::Io {
            relative_path: label.to_string(),
            source,
        })?;
    if !canonical.starts_with(home) {
        return Err(ProfileStartupFileError::UnsafePath {
            relative_path: label.to_string(),
            message: "parent escaped CODEX_HOME".to_string(),
        });
    }
    Ok(canonical)
}

fn relative_path_label(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn validate_identifier(value: &str, label: &str) -> Result<(), ProfileStartupFileError> {
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(ProfileStartupFileError::InvalidInput(format!(
            "{label} must contain 1 to {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
    }) || !value
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(ProfileStartupFileError::InvalidInput(format!(
            "{label} must use lowercase ASCII letters, digits, '-' or '_' and start and end with a letter or digit"
        )));
    }
    Ok(())
}

fn restrict_file_permissions(file: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("open-web-codex-{label}-{}", Uuid::now_v7()));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn materializes_only_native_skill_and_agent_role_paths() {
        let root = temp_root("startup-files");
        let home = root.join("profile");
        let workspace = root.join("workspace");
        fs::create_dir_all(&home).expect("create home");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(home.join("config.toml"), "model = \"keep-me\"\n").expect("write config");
        fs::create_dir_all(home.join("skills/user-skill")).expect("create user skill");
        fs::write(home.join("skills/user-skill/SKILL.md"), "user").expect("write user skill");

        let files = vec![
            ProfileStartupFile::skill("warehouse-supervisor", b"supervisor".to_vec())
                .expect("skill"),
            ProfileStartupFile::agent_role(
                "data_agent",
                b"developer_instructions = \"data\"\n".to_vec(),
            )
            .expect("role"),
        ];
        reconcile_profile_startup_files(&home, &files, &[]).expect("materialize");
        reconcile_profile_startup_files(&home, &files, &[]).expect("idempotent materialize");

        assert_eq!(
            fs::read(home.join("skills/warehouse-supervisor/SKILL.md")).expect("read skill"),
            b"supervisor"
        );
        assert_eq!(
            fs::read(home.join("agents/data_agent.toml")).expect("read role"),
            b"developer_instructions = \"data\"\n"
        );
        assert_eq!(
            fs::read_to_string(home.join("config.toml")).expect("read config"),
            "model = \"keep-me\"\n"
        );
        assert_eq!(
            fs::read_to_string(home.join("skills/user-skill/SKILL.md")).expect("read user skill"),
            "user"
        );
        assert!(fs::read_dir(&workspace)
            .expect("workspace entries")
            .next()
            .is_none());
        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[test]
    fn preserves_existing_profile_content_and_materializes_other_missing_files() {
        let root = temp_root("startup-conflict");
        let home = root.join("profile");
        fs::create_dir_all(home.join("agents")).expect("create agents");
        fs::write(home.join("agents/data_agent.toml"), "profile-owned")
            .expect("write profile role");
        let role =
            ProfileStartupFile::agent_role("data_agent", b"built-in".to_vec()).expect("role");
        let skill = ProfileStartupFile::skill("warehouse-data", b"seed".to_vec()).expect("skill");

        reconcile_profile_startup_files(&home, &[role, skill], &[])
            .expect("materialize missing seed");
        assert_eq!(
            fs::read_to_string(home.join("agents/data_agent.toml")).expect("read profile role"),
            "profile-owned"
        );
        assert_eq!(
            fs::read_to_string(home.join("skills/warehouse-data/SKILL.md")).expect("read skill"),
            "seed"
        );
        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[test]
    fn refreshes_only_managed_package_destinations() {
        let root = temp_root("startup-managed");
        let home = root.join("profile");
        fs::create_dir_all(home.join("agents")).expect("create agents");
        fs::create_dir_all(home.join("skills/warehouse-network")).expect("create package skill");
        fs::create_dir_all(home.join("skills/user-skill")).expect("create user skill");
        fs::write(home.join("agents/network_agent.toml"), "old-role").expect("write old role");
        fs::write(home.join("skills/warehouse-network/SKILL.md"), "old-skill")
            .expect("write old skill");
        fs::write(home.join("skills/user-skill/SKILL.md"), "user-owned").expect("write user skill");
        fs::write(home.join("config.toml"), "model = \"keep-me\"\n").expect("write config");

        let files = [
            ProfileStartupFile::package_skill("warehouse-network", b"current-skill".to_vec())
                .expect("managed skill"),
            ProfileStartupFile::package_agent_role("network_agent", b"current-role".to_vec())
                .expect("managed role"),
        ];
        reconcile_profile_startup_files(&home, &files, &[]).expect("refresh managed files");
        reconcile_profile_startup_files(&home, &files, &[]).expect("idempotent refresh");

        assert_eq!(
            fs::read_to_string(home.join("skills/warehouse-network/SKILL.md"))
                .expect("read managed skill"),
            "current-skill"
        );
        assert_eq!(
            fs::read_to_string(home.join("agents/network_agent.toml")).expect("read managed role"),
            "current-role"
        );
        assert_eq!(
            fs::read_to_string(home.join("skills/user-skill/SKILL.md")).expect("read user skill"),
            "user-owned"
        );
        assert_eq!(
            fs::read_to_string(home.join("config.toml")).expect("read config"),
            "model = \"keep-me\"\n"
        );
        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[test]
    fn rejects_duplicate_destinations_before_writing_any_seed() {
        let root = temp_root("startup-duplicate");
        let home = root.join("profile");
        fs::create_dir_all(&home).expect("create home");
        let role =
            ProfileStartupFile::agent_role("data_agent", b"built-in".to_vec()).expect("role");

        assert!(matches!(
            reconcile_profile_startup_files(&home, &[role.clone(), role], &[]),
            Err(ProfileStartupFileError::InvalidInput(_))
        ));
        assert!(!home.join("agents/data_agent.toml").exists());
        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_and_leaves_no_staging_file() {
        use std::os::unix::fs::symlink;

        let root = temp_root("startup-symlink");
        let home = root.join("profile");
        let outside = root.join("outside");
        fs::create_dir_all(&home).expect("create home");
        fs::create_dir_all(&outside).expect("create outside");
        symlink(&outside, home.join("agents")).expect("create symlink");
        let role =
            ProfileStartupFile::agent_role("data_agent", b"built-in".to_vec()).expect("role");

        assert!(matches!(
            reconcile_profile_startup_files(&home, &[role], &[]),
            Err(ProfileStartupFileError::UnsafePath { .. })
        ));
        assert!(fs::read_dir(&outside)
            .expect("outside entries")
            .next()
            .is_none());
        assert!(fs::read_dir(&home)
            .expect("home entries")
            .all(|entry| !entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .contains(".tmp")));

        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[test]
    fn rejects_first_non_regular_target_before_later_seed() {
        let root = temp_root("startup-directory-target");
        let home = root.join("profile");
        fs::create_dir_all(home.join("agents/data_agent.toml"))
            .expect("create directory at file target");
        let files = [
            ProfileStartupFile::agent_role("data_agent", b"built-in".to_vec()).expect("role"),
            ProfileStartupFile::skill("warehouse-data", b"skill".to_vec()).expect("skill"),
        ];

        assert!(matches!(
            reconcile_profile_startup_files(&home, &files, &[]),
            Err(ProfileStartupFileError::UnsafePath { .. })
        ));
        assert!(!home.join("skills/warehouse-data/SKILL.md").exists());
        assert!(fs::read_dir(home.join("agents"))
            .expect("agent entries")
            .all(|entry| !entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .contains(".tmp")));
        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[test]
    fn unsafe_removal_does_not_publish_an_earlier_managed_update() {
        let root = temp_root("startup-removal-preflight");
        let home = root.join("profile");
        fs::create_dir_all(home.join("skills/managed")).expect("create managed skill");
        fs::create_dir_all(home.join("agents/unsafe.toml"))
            .expect("create directory at removal target");
        fs::write(home.join("skills/managed/SKILL.md"), "old").expect("write old skill");

        let update =
            ProfileStartupFile::package_skill("managed", b"new".to_vec()).expect("managed update");
        let removal =
            ProfileStartupFileRemoval::package_agent_role("unsafe").expect("managed removal");
        assert!(matches!(
            reconcile_profile_startup_files(&home, &[update], &[removal]),
            Err(ProfileStartupFileError::UnsafePath { .. })
        ));
        assert_eq!(
            fs::read_to_string(home.join("skills/managed/SKILL.md")).expect("read managed skill"),
            "old"
        );
        assert!(fs::read_dir(home.join("skills/managed"))
            .expect("managed entries")
            .all(|entry| !entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .contains(".tmp")));
        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[test]
    fn removes_only_exact_managed_package_destinations() {
        let root = temp_root("startup-deactivate");
        let home = root.join("profile");
        fs::create_dir_all(home.join("skills/managed")).expect("create managed skill");
        fs::create_dir_all(home.join("skills/user-skill")).expect("create user skill");
        fs::create_dir_all(home.join("agents")).expect("create agents");
        fs::write(home.join("skills/managed/SKILL.md"), "managed").expect("write managed skill");
        fs::write(home.join("skills/user-skill/SKILL.md"), "user").expect("write user skill");
        fs::write(home.join("agents/managed_agent.toml"), "managed").expect("write managed role");
        fs::write(home.join("agents/user_agent.toml"), "user").expect("write user role");

        let removals = [
            ProfileStartupFileRemoval::package_skill("managed").expect("skill removal"),
            ProfileStartupFileRemoval::package_agent_role("managed_agent").expect("role removal"),
        ];
        reconcile_profile_startup_files(&home, &[], &removals).expect("deactivate package");

        assert!(!home.join("skills/managed/SKILL.md").exists());
        assert!(!home.join("agents/managed_agent.toml").exists());
        assert_eq!(
            fs::read_to_string(home.join("skills/user-skill/SKILL.md")).expect("read user skill"),
            "user"
        );
        assert_eq!(
            fs::read_to_string(home.join("agents/user_agent.toml")).expect("read user role"),
            "user"
        );
        fs::remove_dir_all(root).expect("remove temp root");
    }
}
