//! Profile-host primitives shared by platform runtime adapters.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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

#[cfg(test)]
mod tests {
    use super::ensure_profile_home;
    use std::fs;
    use std::path::PathBuf;
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
}
