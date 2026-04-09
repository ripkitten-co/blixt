/// Password hashing and verification using argon2id.
///
/// Uses the default argon2id parameters from the `argon2` crate, which follow
/// OWASP recommendations. Salts are generated from the OS CSPRNG.
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};

/// Hashes a plaintext password with argon2id and a random salt.
///
/// Returns the PHC-format hash string (e.g. `$argon2id$v=19$...`).
pub fn hash_password(password: &str) -> crate::error::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|err| crate::error::Error::Internal(format!("Hash error: {err}")))?;
    Ok(hash.to_string())
}

/// Verifies a plaintext password against an argon2id PHC-format hash.
///
/// Returns `true` if the password matches, `false` otherwise.
pub fn verify_password(password: &str, hash: &str) -> crate::error::Result<bool> {
    let parsed = argon2::PasswordHash::new(hash)
        .map_err(|err| crate::error::Error::Internal(format!("Invalid hash: {err}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_succeeds() {
        let hash = hash_password("correct-horse-battery-staple").expect("hashing failed");
        let result = verify_password("correct-horse-battery-staple", &hash).expect("verify failed");
        assert!(result);
    }

    #[test]
    fn wrong_password_fails() {
        let hash = hash_password("correct-horse-battery-staple").expect("hashing failed");
        let result = verify_password("wrong-password", &hash).expect("verify failed");
        assert!(!result);
    }

    #[test]
    fn hash_uses_argon2id() {
        let hash = hash_password("test-password").expect("hashing failed");
        assert!(
            hash.starts_with("$argon2id$"),
            "hash should use argon2id, got: {hash}"
        );
    }
}
