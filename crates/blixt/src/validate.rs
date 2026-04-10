//! Input validation with a fluent builder API.
//!
//! Provides chainable validation rules for common types. Errors are
//! collected per-field and returned as [`ValidationErrors`] via
//! [`Error::Validation`].
//!
//! # Example
//!
//! ```rust,ignore
//! use blixt::prelude::*;
//! use blixt::validate::Validator;
//!
//! let mut v = Validator::new();
//! v.str_field(&title, "title").not_empty().max_length(255);
//! v.i64_field(priority, "priority").range(1, 5);
//! v.check()?; // Returns Err(Error::Validation(...)) if any rule fails
//! ```

use crate::error::{Error, ValidationErrors};

/// Built-in regex pattern for email validation.
pub const EMAIL_PATTERN: &str = r"^[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}$";

/// Built-in regex pattern for alphanumeric strings.
pub const ALPHANUMERIC_PATTERN: &str = r"^[a-zA-Z0-9]+$";

/// Built-in regex pattern for URL-safe slugs.
pub const SLUG_PATTERN: &str = r"^[a-z0-9]+(?:-[a-z0-9]+)*$";

/// Collects validation errors across multiple fields.
pub struct Validator {
    errors: ValidationErrors,
    current_field: Option<CurrentField>,
}

struct CurrentField {
    name: &'static str,
    kind: FieldKind,
}

enum FieldKind {
    Str(String),
    I64(i64),
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

impl Validator {
    /// Create a new validator with no errors.
    pub fn new() -> Self {
        Self {
            errors: ValidationErrors::new(),
            current_field: None,
        }
    }

    /// Begin validating a string field.
    pub fn str_field(&mut self, value: &str, name: &'static str) -> &mut Self {
        self.current_field = Some(CurrentField {
            name,
            kind: FieldKind::Str(value.to_owned()),
        });
        self
    }

    /// Begin validating an integer field.
    pub fn i64_field(&mut self, value: i64, name: &'static str) -> &mut Self {
        self.current_field = Some(CurrentField {
            name,
            kind: FieldKind::I64(value),
        });
        self
    }

    // --- String rules ---

    /// Require the string to be non-empty after trimming.
    pub fn not_empty(&mut self) -> &mut Self {
        if let Some(CurrentField {
            name,
            kind: FieldKind::Str(ref v),
        }) = self.current_field
        {
            if v.trim().is_empty() {
                self.errors.add(name, format!("{name} must not be empty"));
            }
        }
        self
    }

    /// Require the string length to be at most `max` characters.
    pub fn max_length(&mut self, max: usize) -> &mut Self {
        if let Some(CurrentField {
            name,
            kind: FieldKind::Str(ref v),
        }) = self.current_field
        {
            if v.len() > max {
                self.errors
                    .add(name, format!("{name} must be at most {max} characters"));
            }
        }
        self
    }

    /// Require the string length to be at least `min` characters.
    pub fn min_length(&mut self, min: usize) -> &mut Self {
        if let Some(CurrentField {
            name,
            kind: FieldKind::Str(ref v),
        }) = self.current_field
        {
            if v.len() < min {
                self.errors
                    .add(name, format!("{name} must be at least {min} characters"));
            }
        }
        self
    }

    /// Require the string to match a regex pattern.
    pub fn pattern(&mut self, regex: &str, message: &str) -> &mut Self {
        if let Some(CurrentField {
            name,
            kind: FieldKind::Str(ref v),
        }) = self.current_field
        {
            match regex::Regex::new(regex) {
                Ok(re) => {
                    if !re.is_match(v) {
                        self.errors.add(name, format!("{name} {message}"));
                    }
                }
                Err(_) => {
                    self.errors
                        .add(name, format!("{name}: invalid validation pattern"));
                }
            }
        }
        self
    }

    // --- Numeric rules ---

    /// Require the integer to be within an inclusive range.
    pub fn range(&mut self, min: i64, max: i64) -> &mut Self {
        if let Some(CurrentField {
            name,
            kind: FieldKind::I64(v),
        }) = self.current_field
        {
            if v < min || v > max {
                self.errors
                    .add(name, format!("{name} must be between {min} and {max}"));
            }
        }
        self
    }

    /// Require the integer to be positive (> 0).
    pub fn positive(&mut self) -> &mut Self {
        if let Some(CurrentField {
            name,
            kind: FieldKind::I64(v),
        }) = self.current_field
        {
            if v <= 0 {
                self.errors.add(name, format!("{name} must be positive"));
            }
        }
        self
    }

    /// Returns `Ok(())` if no errors, or `Err(Error::Validation(...))`.
    pub fn check(self) -> crate::error::Result<()> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Validation(self.errors))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_empty_rejects_empty_string() {
        let mut v = Validator::new();
        v.str_field("", "title").not_empty();
        assert!(v.check().is_err());
    }

    #[test]
    fn not_empty_rejects_whitespace_only() {
        let mut v = Validator::new();
        v.str_field("   ", "title").not_empty();
        assert!(v.check().is_err());
    }

    #[test]
    fn not_empty_accepts_valid_string() {
        let mut v = Validator::new();
        v.str_field("hello", "title").not_empty();
        assert!(v.check().is_ok());
    }

    #[test]
    fn max_length_at_boundary() {
        let mut v = Validator::new();
        v.str_field("abc", "name").max_length(3);
        assert!(v.check().is_ok());
    }

    #[test]
    fn max_length_over_boundary() {
        let mut v = Validator::new();
        v.str_field("abcd", "name").max_length(3);
        assert!(v.check().is_err());
    }

    #[test]
    fn min_length_at_boundary() {
        let mut v = Validator::new();
        v.str_field("ab", "password").min_length(2);
        assert!(v.check().is_ok());
    }

    #[test]
    fn min_length_under_boundary() {
        let mut v = Validator::new();
        v.str_field("a", "password").min_length(2);
        assert!(v.check().is_err());
    }

    #[test]
    fn pattern_valid_email() {
        let mut v = Validator::new();
        v.str_field("user@example.com", "email")
            .pattern(EMAIL_PATTERN, "must be a valid email");
        assert!(v.check().is_ok());
    }

    #[test]
    fn pattern_invalid_email() {
        let mut v = Validator::new();
        v.str_field("not-an-email", "email")
            .pattern(EMAIL_PATTERN, "must be a valid email");
        assert!(v.check().is_err());
    }

    #[test]
    fn pattern_invalid_regex_adds_error() {
        let mut v = Validator::new();
        v.str_field("anything", "field").pattern("[invalid", "msg");
        assert!(v.check().is_err());
    }

    #[test]
    fn range_at_lower_boundary() {
        let mut v = Validator::new();
        v.i64_field(1, "priority").range(1, 5);
        assert!(v.check().is_ok());
    }

    #[test]
    fn range_at_upper_boundary() {
        let mut v = Validator::new();
        v.i64_field(5, "priority").range(1, 5);
        assert!(v.check().is_ok());
    }

    #[test]
    fn range_below_lower_boundary() {
        let mut v = Validator::new();
        v.i64_field(0, "priority").range(1, 5);
        assert!(v.check().is_err());
    }

    #[test]
    fn range_above_upper_boundary() {
        let mut v = Validator::new();
        v.i64_field(6, "priority").range(1, 5);
        assert!(v.check().is_err());
    }

    #[test]
    fn positive_rejects_zero() {
        let mut v = Validator::new();
        v.i64_field(0, "count").positive();
        assert!(v.check().is_err());
    }

    #[test]
    fn positive_rejects_negative() {
        let mut v = Validator::new();
        v.i64_field(-1, "count").positive();
        assert!(v.check().is_err());
    }

    #[test]
    fn positive_accepts_one() {
        let mut v = Validator::new();
        v.i64_field(1, "count").positive();
        assert!(v.check().is_ok());
    }

    #[test]
    fn multiple_fields_collect_errors() {
        let mut v = Validator::new();
        v.str_field("", "title").not_empty();
        v.i64_field(0, "priority").range(1, 5);
        let err = v.check().unwrap_err();
        if let Error::Validation(errors) = err {
            assert!(errors.errors.contains_key("title"));
            assert!(errors.errors.contains_key("priority"));
        } else {
            panic!("expected Validation error");
        }
    }

    #[test]
    fn check_returns_ok_when_no_errors() {
        let v = Validator::new();
        assert!(v.check().is_ok());
    }

    #[test]
    fn error_messages_reference_field_names() {
        let mut v = Validator::new();
        v.str_field("secret_value", "username")
            .not_empty()
            .min_length(50);
        let err = v.check().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("username"));
        assert!(!msg.contains("secret_value"));
    }

    #[test]
    fn chained_string_rules() {
        let mut v = Validator::new();
        v.str_field("", "title").not_empty().max_length(255);
        let err = v.check().unwrap_err();
        if let Error::Validation(errors) = err {
            let title_errors = &errors.errors["title"];
            assert_eq!(title_errors.len(), 1); // only not_empty fires
        } else {
            panic!("expected Validation error");
        }
    }

    #[test]
    fn slug_pattern_valid() {
        let mut v = Validator::new();
        v.str_field("my-cool-slug", "slug")
            .pattern(SLUG_PATTERN, "must be a valid slug");
        assert!(v.check().is_ok());
    }

    #[test]
    fn slug_pattern_invalid() {
        let mut v = Validator::new();
        v.str_field("NOT A SLUG!", "slug")
            .pattern(SLUG_PATTERN, "must be a valid slug");
        assert!(v.check().is_err());
    }
}
