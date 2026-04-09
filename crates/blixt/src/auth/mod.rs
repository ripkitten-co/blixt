/// Authentication and authorization primitives.
///
/// Provides password hashing (argon2id), JWT token management with algorithm
/// pinning, and Axum extractors for deny-by-default route protection.
/// Axum extractors for deny-by-default route protection.
pub mod extractor;
/// JWT token creation and validation.
pub mod jwt;
/// Password hashing and verification (argon2id).
pub mod password;

pub use extractor::{AuthUser, JwtSecret, OptionalAuth};
pub use jwt::Claims;
