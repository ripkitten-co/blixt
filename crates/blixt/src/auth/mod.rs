/// Authentication and authorization primitives.
///
/// Provides password hashing (argon2id), JWT token management with algorithm
/// pinning, and Axum extractors for deny-by-default route protection.
pub mod extractor;
pub mod jwt;
pub mod password;

pub use extractor::{AuthUser, JwtSecret, OptionalAuth};
pub use jwt::Claims;
