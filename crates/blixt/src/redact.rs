use std::fmt;

use serde::de::Deserializer;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

/// A newtype wrapper that prevents the inner value from leaking into logs,
/// debug output, or serialized representations. `Debug`, `Display`, and
/// `Serialize` all emit `[REDACTED]` instead of the real value.
///
/// Use `expose()` or `into_inner()` when you need the actual value for
/// business logic.
pub struct Redact<T>(T);

impl<T> Redact<T> {
    /// Creates a new `Redact` wrapper around the given value.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Returns a reference to the inner value for business logic.
    pub fn expose(&self) -> &T {
        &self.0
    }

    /// Consumes the wrapper and returns the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Clone> Clone for Redact<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: PartialEq> PartialEq for Redact<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> From<T> for Redact<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T> fmt::Debug for Redact<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl<T> fmt::Display for Redact<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl<T> Serialize for Redact<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str("[REDACTED]")
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Redact<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Redact::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_prints_redacted() {
        let secret = Redact::new("password");
        assert_eq!(format!("{:?}", secret), "[REDACTED]");
    }

    #[test]
    fn display_prints_redacted() {
        let secret = Redact::new("secret");
        assert_eq!(format!("{}", secret), "[REDACTED]");
    }

    #[test]
    fn serialize_emits_redacted_string() {
        let secret = Redact::new("secret");
        let json = serde_json::to_string(&secret).expect("serialization should succeed");
        assert_eq!(json, "\"[REDACTED]\"");
    }

    #[test]
    fn deserialize_passes_through_to_inner() {
        let redacted: Redact<String> =
            serde_json::from_str("\"hello\"").expect("deserialization should succeed");
        assert_eq!(redacted.expose(), "hello");
    }

    #[test]
    fn expose_returns_inner_value() {
        let secret = Redact::new("my-secret".to_owned());
        assert_eq!(secret.expose(), "my-secret");
    }

    #[test]
    fn into_inner_unwraps_value() {
        let secret = Redact::new("unwrap-me".to_owned());
        let value = secret.into_inner();
        assert_eq!(value, "unwrap-me");
    }

    #[test]
    fn from_trait_wraps_value() {
        let email: Redact<String> = "user@example.com".to_owned().into();
        assert_eq!(email.expose(), "user@example.com");
        assert_eq!(format!("{}", email), "[REDACTED]");
    }

    #[test]
    fn clone_produces_equal_copy() {
        let original = Redact::new("cloneable".to_owned());
        let cloned = original.clone();
        assert_eq!(original, cloned);
        assert_eq!(cloned.expose(), "cloneable");
    }

    #[test]
    fn debug_never_leaks_inner_value() {
        let secret = Redact::new("super-secret-token");
        let debug_output = format!("{:?}", secret);
        assert!(!debug_output.contains("super-secret-token"));
    }

    #[test]
    fn display_never_leaks_inner_value() {
        let secret = Redact::new("super-secret-token");
        let display_output = format!("{}", secret);
        assert!(!display_output.contains("super-secret-token"));
    }

    #[test]
    fn serialize_never_leaks_inner_value() {
        let secret = Redact::new("super-secret-token".to_owned());
        let json = serde_json::to_string(&secret).expect("serialization should succeed");
        assert!(!json.contains("super-secret-token"));
    }

    #[test]
    fn deserialize_numeric_value() {
        let redacted: Redact<u64> =
            serde_json::from_str("12345").expect("deserialization should succeed");
        assert_eq!(*redacted.expose(), 12345_u64);
    }
}
