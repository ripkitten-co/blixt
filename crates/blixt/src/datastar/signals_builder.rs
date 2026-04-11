use serde::Serialize;
use serde_json::{Map, Value};

/// Builder for Datastar signal payloads.
pub struct Signals {
    map: Map<String, Value>,
}

impl Signals {
    /// Create an empty signal payload.
    pub fn new() -> Self {
        Self { map: Map::new() }
    }

    /// Insert a key-value pair into the signal payload.
    pub fn set(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.map.insert(key.to_owned(), value.into());
        self
    }

    /// Create a payload where every key is set to an empty string.
    pub fn clear(keys: &[&str]) -> Self {
        let mut s = Self::new();
        for key in keys {
            s.map
                .insert((*key).to_owned(), Value::String(String::new()));
        }
        s
    }
}

impl Default for Signals {
    fn default() -> Self {
        Self::new()
    }
}

impl Serialize for Signals {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        self.map.serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_sets_all_keys_to_empty_string() {
        let s = Signals::clear(&["title", "body"]);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["title"], "");
        assert_eq!(json["body"], "");
    }

    #[test]
    fn set_accepts_string() {
        let s = Signals::new().set("name", "hello");
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["name"], "hello");
    }

    #[test]
    fn set_accepts_bool() {
        let s = Signals::new().set("active", false);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["active"], false);
    }

    #[test]
    fn set_accepts_i64() {
        let s = Signals::new().set("count", 42);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["count"], 42);
    }

    #[test]
    fn set_accepts_f64() {
        let s = Signals::new().set("price", 9.99);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["price"], 9.99);
    }

    #[test]
    fn chaining_multiple_sets() {
        let s = Signals::new()
            .set("title", "")
            .set("count", 0)
            .set("active", false);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["title"], "");
        assert_eq!(json["count"], 0);
        assert_eq!(json["active"], false);
    }

    #[test]
    fn clear_with_empty_slice() {
        let s = Signals::clear(&[]);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }
}
