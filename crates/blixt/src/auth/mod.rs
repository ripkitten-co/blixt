//! Authentication and authorization primitives.
//!
//! Provides password hashing (argon2id), JWT token management with algorithm
//! pinning, HttpOnly cookie session management, and Axum extractors for
//! deny-by-default route protection.

/// Auth cookie management (set/clear HttpOnly session cookies).
pub mod cookie;
/// Axum extractors for deny-by-default route protection.
pub mod extractor;
/// JWT token creation and validation.
pub mod jwt;
/// Password hashing and verification (argon2id).
pub mod password;

pub use cookie::AUTH_COOKIE_NAME;
pub use extractor::{AuthUser, JwtSecret, OptionalAuth};
pub use jwt::Claims;

/// Hashes the input with SHA-256 and returns the hex-encoded digest.
///
/// Used for hashing tokens before database storage so that a DB compromise
/// doesn't reveal raw bearer tokens.
pub fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(input.as_bytes());
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_known_digest() {
        // echo -n "hello" | sha256sum
        let expected = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert_eq!(sha256_hex("hello"), expected);
    }

    #[test]
    fn sha256_hex_empty_input() {
        let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(sha256_hex(""), expected);
    }

    #[test]
    fn sha256_hex_returns_lowercase_hex() {
        let result = sha256_hex("test");
        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(result.chars().all(|c| !c.is_ascii_uppercase()));
    }
}
