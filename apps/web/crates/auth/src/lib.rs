//! Argon2id password hashing and verification for platform identities.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use std::sync::OnceLock;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("password must not be empty")]
    Empty,
    #[error("password hashing failed")]
    Hash,
}

pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    if password.is_empty() {
        return Err(PasswordError::Empty);
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| PasswordError::Hash)
}

pub fn verify_password(password: &str, encoded: &str) -> bool {
    PasswordHash::new(encoded).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}

/// Verify an optional account hash while still doing an Argon2 verification
/// for unknown accounts, reducing account-existence timing differences.
pub fn verify_password_or_dummy(password: &str, encoded: Option<&str>) -> bool {
    static DUMMY_HASH: OnceLock<String> = OnceLock::new();
    let exists = encoded.is_some();
    let encoded = encoded.unwrap_or_else(|| {
        DUMMY_HASH
            .get_or_init(|| {
                hash_password("open-web-codex-unavailable-account")
                    .expect("static dummy password is valid")
            })
            .as_str()
    });
    verify_password(password, encoded) && exists
}

#[cfg(test)]
mod tests {
    use super::{hash_password, verify_password, verify_password_or_dummy};

    #[test]
    fn hashes_with_argon2id_and_verifies_without_exposing_the_password() {
        let password = "correct horse battery staple";
        let encoded = hash_password(password).expect("hash password");

        assert!(encoded.starts_with("$argon2id$"));
        assert!(!encoded.contains(password));
        assert!(verify_password(password, &encoded));
        assert!(!verify_password("wrong", &encoded));
    }

    #[test]
    fn rejects_legacy_sha256_password_hashes() {
        assert!(!verify_password(
            "legacy-password",
            "11b564e7b4ba0b765a5d7b11d8e29b3bfbaad4249fefee3134523245640491fc"
        ));
    }

    #[test]
    fn unknown_accounts_use_the_dummy_verification_path() {
        assert!(!verify_password_or_dummy("anything", None));
    }
}
