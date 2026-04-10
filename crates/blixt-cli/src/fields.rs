use crate::validate;

const RESERVED_FIELD_NAMES: &[&str] = &["id", "created_at", "updated_at"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldType {
    String,
    Int,
    Bool,
    Float,
}

impl FieldType {
    pub fn parse(input: &str) -> Result<Self, String> {
        match input {
            "string" | "text" => Ok(Self::String),
            "int" | "integer" => Ok(Self::Int),
            "bool" | "boolean" => Ok(Self::Bool),
            "float" => Ok(Self::Float),
            other => Err(format!(
                "Unknown field type '{other}'. Supported: string, text, int, integer, bool, boolean, float"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbDialect {
    Postgres,
    Sqlite,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    pub name: String,
    pub field_type: FieldType,
}

impl FieldDef {
    pub fn rust_type(&self) -> &'static str {
        match self.field_type {
            FieldType::String => "String",
            FieldType::Int => "i64",
            FieldType::Bool => "bool",
            FieldType::Float => "f64",
        }
    }

    pub fn sql_type(&self, dialect: DbDialect) -> &'static str {
        match (self.field_type, dialect) {
            (FieldType::String, _) => "TEXT NOT NULL",
            (FieldType::Int, DbDialect::Postgres) => "BIGINT NOT NULL DEFAULT 0",
            (FieldType::Int, DbDialect::Sqlite) => "INTEGER NOT NULL DEFAULT 0",
            (FieldType::Bool, DbDialect::Postgres) => "BOOLEAN NOT NULL DEFAULT FALSE",
            (FieldType::Bool, DbDialect::Sqlite) => "BOOLEAN NOT NULL DEFAULT 0",
            (FieldType::Float, DbDialect::Postgres) => "DOUBLE PRECISION NOT NULL DEFAULT 0",
            (FieldType::Float, DbDialect::Sqlite) => "REAL NOT NULL DEFAULT 0",
        }
    }

    pub fn is_string(&self) -> bool {
        self.field_type == FieldType::String
    }
}

pub fn parse_fields(args: &[&str]) -> Result<Vec<FieldDef>, String> {
    let mut fields = Vec::new();
    let mut seen_names = Vec::new();

    for arg in args {
        let Some((raw_name, raw_type)) = arg.split_once(':') else {
            return Err(format!("Expected format 'field:type', got '{arg}'"));
        };

        let name = validate::to_snake_case(raw_name);
        validate::validate_name(&name)?;

        if RESERVED_FIELD_NAMES.contains(&name.as_str()) {
            return Err(format!("Field name '{name}' is reserved"));
        }

        if seen_names.contains(&name) {
            return Err(format!("Duplicate field name '{name}'"));
        }

        let field_type = FieldType::parse(raw_type)?;
        seen_names.push(name.clone());
        fields.push(FieldDef { name, field_type });
    }

    Ok(fields)
}

pub fn detect_dialect_from_url(url: &str) -> DbDialect {
    if url.starts_with("sqlite") {
        DbDialect::Sqlite
    } else {
        DbDialect::Postgres
    }
}

pub fn detect_dialect() -> DbDialect {
    dotenvy::dotenv().ok();
    match std::env::var("DATABASE_URL") {
        Ok(url) => detect_dialect_from_url(&url),
        Err(_) => DbDialect::Postgres,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_field_definitions() {
        let result = parse_fields(&["title:string", "count:int", "active:bool"]).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "title");
        assert_eq!(result[0].field_type, FieldType::String);
        assert_eq!(result[1].name, "count");
        assert_eq!(result[1].field_type, FieldType::Int);
        assert_eq!(result[2].name, "active");
        assert_eq!(result[2].field_type, FieldType::Bool);
    }

    #[test]
    fn accepts_type_aliases() {
        let result = parse_fields(&["body:text", "age:integer", "published:boolean"]).unwrap();
        assert_eq!(result[0].field_type, FieldType::String);
        assert_eq!(result[1].field_type, FieldType::Int);
        assert_eq!(result[2].field_type, FieldType::Bool);
    }

    #[test]
    fn rejects_unknown_type() {
        let err = parse_fields(&["name:varchar"]).unwrap_err();
        assert!(err.contains("Unknown field type"), "got: {err}");
    }

    #[test]
    fn rejects_missing_colon() {
        let err = parse_fields(&["titlestring"]).unwrap_err();
        assert!(err.contains("Expected format"), "got: {err}");
    }

    #[test]
    fn rejects_reserved_field_names() {
        for name in &["id:int", "created_at:string", "updated_at:string"] {
            let err = parse_fields(&[name]).unwrap_err();
            assert!(
                err.contains("reserved"),
                "expected reserved error for {name}, got: {err}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_field_names() {
        let err = parse_fields(&["title:string", "title:text"]).unwrap_err();
        assert!(err.contains("Duplicate"), "got: {err}");
    }

    #[test]
    fn rejects_rust_keywords() {
        let err = parse_fields(&["type:string"]).unwrap_err();
        assert!(err.contains("reserved Rust keyword"), "got: {err}");

        let err = parse_fields(&["fn:int"]).unwrap_err();
        assert!(err.contains("reserved Rust keyword"), "got: {err}");
    }

    #[test]
    fn empty_fields_returns_empty_vec() {
        let result = parse_fields(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn field_def_rust_type() {
        let field = FieldDef {
            name: "score".into(),
            field_type: FieldType::Float,
        };
        assert_eq!(field.rust_type(), "f64");
    }

    #[test]
    fn field_def_postgres_sql() {
        let field = FieldDef {
            name: "active".into(),
            field_type: FieldType::Bool,
        };
        assert_eq!(
            field.sql_type(DbDialect::Postgres),
            "BOOLEAN NOT NULL DEFAULT FALSE"
        );
    }

    #[test]
    fn field_def_sqlite_sql() {
        let field = FieldDef {
            name: "count".into(),
            field_type: FieldType::Int,
        };
        assert_eq!(
            field.sql_type(DbDialect::Sqlite),
            "INTEGER NOT NULL DEFAULT 0"
        );
    }

    #[test]
    fn detects_postgres_from_url() {
        assert_eq!(
            detect_dialect_from_url("postgres://localhost/mydb"),
            DbDialect::Postgres
        );
        assert_eq!(
            detect_dialect_from_url("postgresql://localhost/mydb"),
            DbDialect::Postgres
        );
    }

    #[test]
    fn detects_sqlite_from_url() {
        assert_eq!(
            detect_dialect_from_url("sqlite://./data.db"),
            DbDialect::Sqlite
        );
        assert_eq!(
            detect_dialect_from_url("sqlite:data.db?mode=rwc"),
            DbDialect::Sqlite
        );
    }

    #[test]
    fn defaults_to_postgres_for_unknown_url() {
        assert_eq!(
            detect_dialect_from_url("mysql://localhost/db"),
            DbDialect::Postgres
        );
    }
}
