//! Input validation with a fluent, type-safe builder API.
//!
//! Provides chainable validation rules for common types. Each field type
//! returns its own validator, so string rules cannot be called on integer
//! fields (and vice versa) -- misuse is caught at compile time.
//!
//! Errors are collected per-field and returned as [`ValidationErrors`] via
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
//!
//! Calling string rules on an integer field is a compile error:
//!
//! ```compile_fail
//! use blixt::validate::Validator;
//!
//! let mut v = Validator::new();
//! v.i64_field(42, "count").not_empty(); // ERROR: no method `not_empty` on `I64FieldValidator`
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
        }
    }

    /// Begin validating a string field.
    pub fn str_field(&mut self, value: &str, name: &'static str) -> StrFieldValidator<'_> {
        StrFieldValidator {
            validator: self,
            name,
            value: value.to_owned(),
        }
    }

    /// Begin validating an integer field.
    pub fn i64_field(&mut self, value: i64, name: &'static str) -> I64FieldValidator<'_> {
        I64FieldValidator {
            validator: self,
            name,
            value,
        }
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

/// Type-safe validator for string fields.
pub struct StrFieldValidator<'a> {
    validator: &'a mut Validator,
    name: &'static str,
    value: String,
}

impl<'a> StrFieldValidator<'a> {
    /// Require the string to be non-empty after trimming.
    pub fn not_empty(&mut self) -> &mut Self {
        if self.value.trim().is_empty() {
            self.validator
                .errors
                .add(self.name, format!("{} must not be empty", self.name));
        }
        self
    }

    /// Require the string length to be at most `max` characters.
    pub fn max_length(&mut self, max: usize) -> &mut Self {
        if self.value.len() > max {
            self.validator.errors.add(
                self.name,
                format!("{} must be at most {max} characters", self.name),
            );
        }
        self
    }

    /// Require the string length to be at least `min` characters.
    pub fn min_length(&mut self, min: usize) -> &mut Self {
        if self.value.len() < min {
            self.validator.errors.add(
                self.name,
                format!("{} must be at least {min} characters", self.name),
            );
        }
        self
    }

    /// Require the string to match a regex pattern.
    pub fn pattern(&mut self, regex: &str, message: &str) -> &mut Self {
        match regex::Regex::new(regex) {
            Ok(re) => {
                if !re.is_match(&self.value) {
                    self.validator
                        .errors
                        .add(self.name, format!("{} {message}", self.name));
                }
            }
            Err(_) => {
                self.validator.errors.add(
                    self.name,
                    format!("{}: invalid validation pattern", self.name),
                );
            }
        }
        self
    }
}

/// Type-safe validator for i64 fields.
pub struct I64FieldValidator<'a> {
    validator: &'a mut Validator,
    name: &'static str,
    value: i64,
}

impl<'a> I64FieldValidator<'a> {
    /// Require the integer to be within an inclusive range.
    pub fn range(&mut self, min: i64, max: i64) -> &mut Self {
        if self.value < min || self.value > max {
            self.validator.errors.add(
                self.name,
                format!("{} must be between {min} and {max}", self.name),
            );
        }
        self
    }

    /// Require the integer to be positive (> 0).
    pub fn positive(&mut self) -> &mut Self {
        if self.value <= 0 {
            self.validator
                .errors
                .add(self.name, format!("{} must be positive", self.name));
        }
        self
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
