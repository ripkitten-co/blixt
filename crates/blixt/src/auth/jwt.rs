/// JWT token creation and validation with algorithm pinning.
///
/// The algorithm is hardcoded to HS256 and the validation explicitly restricts
/// the accepted algorithm set to `[HS256]`, preventing `none`-algorithm attacks
/// and key-confusion attacks.
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

/// JWT claims payload.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Subject — the user ID.
    pub sub: String,
    /// Expiry as a Unix timestamp (seconds).
    pub exp: usize,
    /// Issued-at as a Unix timestamp (seconds).
    pub iat: usize,
    /// Optional user role for authorization decisions.
    pub role: Option<String>,
}

/// The only algorithm accepted by this module.
const ALGORITHM: Algorithm = Algorithm::HS256;

/// Minimum length for the HMAC secret (bytes).
const MIN_SECRET_LEN: usize = 32;

/// Creates a signed JWT for the given user.
///
/// Returns an error if `secret` is shorter than 32 bytes.
pub fn create_token(
    user_id: &str,
    role: Option<&str>,
    secret: &str,
    ttl_secs: u64,
) -> crate::error::Result<String> {
    if secret.len() < MIN_SECRET_LEN {
        return Err(crate::error::Error::Internal(
            "JWT secret must be at least 32 bytes".into(),
        ));
    }

    let now = now_unix_secs()?;
    let claims = Claims {
        sub: user_id.to_string(),
        exp: now + ttl_secs as usize,
        iat: now,
        role: role.map(String::from),
    };

    let header = Header::new(ALGORITHM);
    encode(
        &header,
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|err| crate::error::Error::Internal(format!("JWT encode error: {err}")))
}

/// Validates a JWT and returns the decoded claims.
///
/// Rejects tokens that use any algorithm other than HS256, are expired,
/// or have an invalid signature.
pub fn validate_token(token: &str, secret: &str) -> crate::error::Result<Claims> {
    let mut validation = Validation::new(ALGORITHM);
    validation.validate_exp = true;
    validation.leeway = 0;
    validation.algorithms = vec![ALGORITHM];

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|_err| crate::error::Error::Unauthorized)
}

/// Returns the current time as Unix seconds.
fn now_unix_secs() -> crate::error::Result<usize> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as usize)
        .map_err(|err| crate::error::Error::Internal(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-that-is-at-least-32-bytes-long!";

    #[test]
    fn create_and_validate_roundtrip() {
        let token =
            create_token("user-42", Some("admin"), SECRET, 3600).expect("token creation failed");
        let claims = validate_token(&token, SECRET).expect("validation failed");

        assert_eq!(claims.sub, "user-42");
        assert_eq!(claims.role.as_deref(), Some("admin"));
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn expired_token_is_rejected() {
        // Create a token that expired 1 second ago.
        let token = create_token("user-42", None, SECRET, 0).expect("token creation failed");
        // The token exp == iat, so it should be expired by validation time.
        std::thread::sleep(std::time::Duration::from_secs(1));
        let result = validate_token(&token, SECRET);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let other_secret = "another-secret-at-least-32-bytes-long!!";
        let token = create_token("user-42", None, SECRET, 3600).expect("token creation failed");
        let result = validate_token(&token, other_secret);
        assert!(result.is_err());
    }

    #[test]
    fn short_secret_is_rejected() {
        let result = create_token("user-42", None, "too-short", 3600);
        assert!(result.is_err());
    }

    #[test]
    fn tampered_payload_is_rejected() {
        let token = create_token("user-42", None, SECRET, 3600).expect("token creation failed");

        // Tamper with the payload segment (second part of the JWT).
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT should have 3 parts");

        // Flip one character in the payload to invalidate the signature.
        let mut payload_bytes = parts[1].as_bytes().to_vec();
        payload_bytes[0] ^= 0xFF;
        let tampered_payload = String::from_utf8_lossy(&payload_bytes);
        let tampered_token = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);

        let result = validate_token(&tampered_token, SECRET);
        assert!(result.is_err());
    }
}
