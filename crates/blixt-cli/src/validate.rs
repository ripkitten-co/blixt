use regex::Regex;

/// Rust reserved keywords that cannot be used as identifiers.
const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "yield",
];

const MAX_NAME_LENGTH: usize = 64;

/// Validates a name for use as a Rust identifier (project, controller, model, etc.).
///
/// Returns the validated name unchanged, or an error message describing
/// why validation failed.
pub fn validate_name(input: &str) -> Result<String, String> {
    if input.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if input.len() > MAX_NAME_LENGTH {
        return Err(format!(
            "Name exceeds maximum length of {MAX_NAME_LENGTH} characters"
        ));
    }
    let pattern = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_]*$")
        .map_err(|err| format!("Internal regex error: {err}"))?;
    if !pattern.is_match(input) {
        return Err(format!(
            "Name '{input}' is invalid: must start with a letter and \
             contain only letters, digits, or underscores"
        ));
    }
    if RUST_KEYWORDS.contains(&input) {
        return Err(format!("Name '{input}' is a reserved Rust keyword"));
    }
    Ok(input.to_string())
}

/// Converts a name to `snake_case`.
///
/// Handles PascalCase, camelCase, and already-snake_case inputs.
/// Consecutive uppercase letters (acronyms) are lowered as a group.
#[allow(dead_code)]
pub fn to_snake_case(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    let chars: Vec<char> = name.chars().collect();

    for (index, &current) in chars.iter().enumerate() {
        if current == '_' {
            result.push('_');
            continue;
        }
        if current.is_uppercase() && index > 0 {
            let prev = chars[index - 1];
            let next_is_lower = chars.get(index + 1).is_some_and(|c| c.is_lowercase());
            if prev.is_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_uppercase() && next_is_lower)
            {
                result.push('_');
            }
        }
        result.push(current.to_lowercase().next().unwrap_or(current));
    }
    result
}

/// Converts a name to `PascalCase`.
///
/// Handles snake_case and already-PascalCase inputs.
#[allow(dead_code)]
pub fn to_pascal_case(name: &str) -> String {
    name.split('_')
        .filter(|segment| !segment.is_empty())
        .map(capitalize_first)
        .collect()
}

/// Pluralizes an English noun using common suffix rules.
///
/// Covers regular nouns, sibilant endings (s/x/z/sh/ch → es),
/// consonant+y → ies, and vowel+y → s.
#[allow(dead_code)]
pub fn pluralize(word: &str) -> String {
    if word.ends_with('s')
        || word.ends_with('x')
        || word.ends_with('z')
        || word.ends_with("sh")
        || word.ends_with("ch")
    {
        return format!("{word}es");
    }

    if let Some(stem) = word.strip_suffix('y') {
        if stem.ends_with(|c: char| !"aeiou".contains(c)) {
            return format!("{stem}ies");
        }
        return format!("{word}s");
    }

    format!("{word}s")
}

/// Capitalizes the first character of a string slice.
fn capitalize_first(segment: &str) -> String {
    let mut chars = segment.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            upper + &chars.as_str().to_lowercase()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names_pass() {
        assert!(validate_name("User").is_ok());
        assert!(validate_name("user_profile").is_ok());
        assert!(validate_name("MyApp123").is_ok());
    }

    #[test]
    fn empty_name_rejected() {
        let result = validate_name("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn injection_strings_rejected() {
        assert!(validate_name("foo; rm -rf /").is_err());
        assert!(validate_name("foo\nbar").is_err());
        assert!(validate_name("a b c").is_err());
        assert!(validate_name("123start").is_err());
    }

    #[test]
    fn rust_keywords_rejected() {
        assert!(validate_name("struct").is_err());
        assert!(validate_name("fn").is_err());
        assert!(validate_name("impl").is_err());
    }

    #[test]
    fn name_exceeding_max_length_rejected() {
        let long_name = "a".repeat(65);
        let result = validate_name(&long_name);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("maximum length"));
    }

    #[test]
    fn exactly_max_length_accepted() {
        let name = "a".repeat(64);
        assert!(validate_name(&name).is_ok());
    }

    #[test]
    fn to_snake_case_pascal() {
        assert_eq!(to_snake_case("UserProfile"), "user_profile");
    }

    #[test]
    fn to_snake_case_acronym() {
        assert_eq!(to_snake_case("HTMLParser"), "html_parser");
    }

    #[test]
    fn to_snake_case_already_snake() {
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    #[test]
    fn to_snake_case_single_word() {
        assert_eq!(to_snake_case("User"), "user");
    }

    #[test]
    fn to_pascal_case_from_snake() {
        assert_eq!(to_pascal_case("user_profile"), "UserProfile");
    }

    #[test]
    fn to_pascal_case_already_pascal() {
        assert_eq!(to_pascal_case("User"), "User");
    }

    #[test]
    fn to_pascal_case_single_word() {
        assert_eq!(to_pascal_case("user"), "User");
    }

    #[test]
    fn pluralize_regular_nouns() {
        assert_eq!(pluralize("post"), "posts");
        assert_eq!(pluralize("user"), "users");
    }

    #[test]
    fn pluralize_s_x_z_sh_ch_endings() {
        assert_eq!(pluralize("status"), "statuses");
        assert_eq!(pluralize("box"), "boxes");
        assert_eq!(pluralize("wish"), "wishes");
        assert_eq!(pluralize("match"), "matches");
        assert_eq!(pluralize("address"), "addresses");
    }

    #[test]
    fn pluralize_consonant_y() {
        assert_eq!(pluralize("category"), "categories");
        assert_eq!(pluralize("city"), "cities");
        assert_eq!(pluralize("company"), "companies");
    }

    #[test]
    fn pluralize_vowel_y() {
        assert_eq!(pluralize("key"), "keys");
        assert_eq!(pluralize("day"), "days");
        assert_eq!(pluralize("toy"), "toys");
    }
}
