//! Profile-host primitives shared by platform runtime adapters.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// Canonical layout for a per-user Profile home under a data root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileLayout {
    pub root: PathBuf,
    pub home: PathBuf,
    pub runtime: PathBuf,
    pub lock_file: PathBuf,
}

impl ProfileLayout {
    pub fn for_user(data_root: &Path, user_id: &str) -> Self {
        let root = data_root.join("profiles").join(user_id);
        Self {
            home: root.join("home"),
            runtime: root.join("runtime"),
            lock_file: root.join("runtime").join("profile.lock"),
            root,
        }
    }

    pub fn ensure_directories(&self) -> io::Result<()> {
        fs::create_dir_all(&self.home)?;
        fs::create_dir_all(&self.runtime)?;
        Ok(())
    }
}

/// Exclusive profile lock backed by a lock file.
pub struct ProfileLock {
    path: PathBuf,
    _file: std::fs::File,
}

impl ProfileLock {
    pub fn acquire(layout: &ProfileLayout) -> io::Result<Self> {
        layout.ensure_directories()?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&layout.lock_file)?;
        Ok(Self {
            path: layout.lock_file.clone(),
            _file: file,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ProfileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Creates a missing Profile home and returns its canonical directory path.
///
/// The Host must call this before spawning Codex. Codex itself intentionally
/// treats a configured but missing `CODEX_HOME` as invalid.
pub fn ensure_profile_home(path: &Path) -> io::Result<PathBuf> {
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Profile home {} is not a directory", path.display()),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
        }
        Err(error) => return Err(error),
    }

    let canonical_path = path.canonicalize()?;
    if !canonical_path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Profile home {} is not a directory", path.display()),
        ));
    }
    Ok(canonical_path)
}

/// Provisions a Profile layout and acquires the single-primary lock.
pub fn provision_profile(
    data_root: &Path,
    user_id: &str,
) -> io::Result<(ProfileLayout, ProfileLock, PathBuf)> {
    let layout = ProfileLayout::for_user(data_root, user_id);
    layout.ensure_directories()?;
    let home = ensure_profile_home(&layout.home)?;
    let lock = ProfileLock::acquire(&layout)?;
    Ok((layout, lock, home))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_path(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "open-web-codex-profile-host-{name}-{}-{timestamp}",
            std::process::id()
        ))
    }

    #[test]
    fn creates_a_missing_profile_home() {
        let path = temporary_path("missing");
        let resolved = ensure_profile_home(&path).expect("create profile home");

        assert!(resolved.is_dir());
        assert_eq!(resolved, path.canonicalize().expect("canonical path"));

        fs::remove_dir_all(path).expect("remove profile home");
    }

    #[test]
    fn rejects_a_profile_home_that_is_a_file() {
        let path = temporary_path("file");
        fs::write(&path, "not a directory").expect("create file");

        let error = ensure_profile_home(&path).expect_err("file cannot be a profile home");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);

        fs::remove_file(path).expect("remove file");
    }

    #[test]
    fn profile_lock_rejects_a_second_holder() {
        let root = temporary_path("lock");
        let layout = ProfileLayout::for_user(&root, "user-a");
        let _first = ProfileLock::acquire(&layout).expect("first lock");
        let second = ProfileLock::acquire(&layout);
        assert!(second.is_err());
        drop(_first);
        fs::remove_dir_all(root).expect("cleanup");
    }
}
