use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use console::style;

use crate::fields::{FieldDef, FieldType, parse_fields};
use crate::validate::{to_pascal_case, to_snake_case};

/// Generates a controller with Askama template views.
///
/// Creates the controller Rust file and corresponding HTML templates
/// for index and show actions under the current working directory.
pub fn generate_controller(name: &str) -> Result<(), String> {
    let base = current_dir()?;
    generate_controller_in(&base, name)
}

/// Generates a model with a database migration.
///
/// Creates the model Rust file with SQLx derive macros and a
/// timestamped SQL migration for creating the table.
pub fn generate_model(name: &str, field_args: &[&str]) -> Result<(), String> {
    let base = current_dir()?;
    let fields = parse_fields(field_args)?;
    generate_model_in(&base, name, &fields)
}

/// Generates a full CRUD scaffold: controller, model, and list fragment.
///
/// Combines controller and model generation, then adds a Datastar-ready
/// list fragment template for streaming updates.
pub fn generate_scaffold(name: &str, field_args: &[&str]) -> Result<(), String> {
    let base = current_dir()?;
    let fields = parse_fields(field_args)?;
    let fields = if fields.is_empty() {
        vec![FieldDef {
            name: "name".into(),
            field_type: FieldType::String,
        }]
    } else {
        fields
    };
    generate_scaffold_in(&base, name, &fields)
}

// --- Path-aware implementations (testable without chdir) ---

/// Controller generation rooted at `base`.
fn generate_controller_in(base: &Path, name: &str) -> Result<(), String> {
    let snake = to_snake_case(name);
    let pascal = to_pascal_case(name);

    write_controller_file(base, &snake, &pascal)?;
    write_index_template(base, &snake, &pascal)?;
    write_show_template(base, &snake, &pascal)?;

    print_controller_route_hint(&snake);
    Ok(())
}

/// Model generation rooted at `base`.
fn generate_model_in(base: &Path, name: &str, fields: &[FieldDef]) -> Result<(), String> {
    let snake = to_snake_case(name);
    let pascal = to_pascal_case(name);
    let plural = format!("{snake}s");

    write_model_file(base, &snake, &pascal, fields)?;
    write_migration_file(base, &snake, &plural, fields)?;

    println!(
        "  {} model {} and migration for {plural}",
        style("created").green().bold(),
        snake
    );
    Ok(())
}

/// Scaffold generation rooted at `base`.
fn generate_scaffold_in(base: &Path, name: &str, fields: &[FieldDef]) -> Result<(), String> {
    let snake = to_snake_case(name);
    let pascal = to_pascal_case(name);

    generate_model_in(base, name, fields)?;
    write_scaffold_controller_file(base, &snake, &pascal, fields)?;
    write_scaffold_index_template(base, &snake, &pascal, fields)?;
    write_scaffold_show_template(base, &snake, &pascal, fields)?;
    write_list_fragment(base, &snake, fields)?;

    println!("  {} controller {snake}", style("created").green().bold());
    print_scaffold_route_hints(&snake);
    Ok(())
}

// --- File writers (private helpers) ---

/// Writes the controller Rust source file.
fn write_controller_file(base: &Path, snake: &str, pascal: &str) -> Result<(), String> {
    let dir = base.join("src/controllers");
    let path = dir.join(format!("{snake}.rs"));
    let content = format!(
        r#"use blixt::prelude::*;

#[derive(Template)]
#[template(path = "pages/{snake}/index.html")]
pub struct {pascal}Index {{
    pub items: Vec<String>,
}}

pub async fn index() -> Result<{pascal}Index> {{
    Ok({pascal}Index {{
        items: vec![],
    }})
}}

#[derive(Template)]
#[template(path = "pages/{snake}/show.html")]
pub struct {pascal}Show {{
    pub id: String,
}}

pub async fn show(Path(id): Path<String>) -> Result<{pascal}Show> {{
    Ok({pascal}Show {{ id }})
}}
"#
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

/// Writes a scaffold controller with database-backed CRUD actions.
fn write_scaffold_controller_file(
    base: &Path,
    snake: &str,
    pascal: &str,
    fields: &[FieldDef],
) -> Result<(), String> {
    let dir = base.join("src/controllers");
    let path = dir.join(format!("{snake}.rs"));

    let col_list = build_column_list(fields).join(", ");

    let signal_extractions: String = fields
        .iter()
        .map(|f| {
            format!(
                "    let {}: {} = signals.get(\"{}\")?;\n",
                f.name,
                f.rust_type(),
                f.name
            )
        })
        .collect();

    let validator_calls: String = fields
        .iter()
        .filter(|f| f.is_string())
        .map(|f| {
            format!(
                "    v.str_field(&{name}, \"{name}\").not_empty().max_length(255);\n",
                name = f.name
            )
        })
        .collect();

    let create_args: String = fields
        .iter()
        .map(|f| {
            if f.is_string() {
                format!(", &{}", f.name)
            } else {
                format!(", {}", f.name)
            }
        })
        .collect();

    let signal_reset_pairs: String = fields
        .iter()
        .map(|f| match f.field_type {
            FieldType::String => format!("\"{}\": \"\"", f.name),
            FieldType::Bool => format!("\"{}\": false", f.name),
            FieldType::Int => format!("\"{}\": 0", f.name),
            FieldType::Float => format!("\"{}\": 0.0", f.name),
        })
        .collect::<Vec<_>>()
        .join(", ");

    let content = format!(
        r#"use blixt::prelude::*;
use blixt::validate::Validator;
use serde_json::json;
use crate::models::{snake}::{pascal};

const PER_PAGE: u32 = 10;

#[derive(Template)]
#[template(path = "pages/{snake}/index.html")]
pub struct {pascal}Index {{
    pub page: Paginated<{pascal}>,
}}

#[derive(Template)]
#[template(path = "fragments/{snake}/list.html")]
pub struct {pascal}ListFragment {{
    pub page: Paginated<{pascal}>,
}}

#[derive(Template)]
#[template(path = "pages/{snake}/show.html")]
pub struct {pascal}Show {{
    pub item: {pascal},
}}

async fn fetch_page(pool: &DbPool, page_num: u32) -> Result<Paginated<{pascal}>> {{
    Paginated::<{pascal}>::query(
        "SELECT {col_list} FROM {snake}s ORDER BY id DESC",
        pool,
        &PaginationParams::new(page_num, PER_PAGE),
    )
    .await
}}

pub async fn index(
    State(ctx): State<AppContext>,
    pagination: PaginationParams,
) -> Result<impl IntoResponse> {{
    let page = fetch_page(&ctx.db, pagination.page()).await?;
    let html = {pascal}Index {{ page }}
        .render()
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(Html(html))
}}

pub async fn show(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse> {{
    let item = {pascal}::find_by_id(&ctx.db, id).await?;
    let html = {pascal}Show {{ item }}
        .render()
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(Html(html))
}}

pub async fn create(
    State(ctx): State<AppContext>,
    signals: DatastarSignals,
) -> Result<impl IntoResponse> {{
{signal_extractions}
    let mut v = Validator::new();
{validator_calls}    v.check()?;

    {pascal}::create(&ctx.db{create_args}).await?;
    let page = fetch_page(&ctx.db, 1).await?;
    SseResponse::new()
        .patch({pascal}ListFragment {{ page }})?
        .signals(&json!({{{signal_reset_pairs}}}))
}}

pub async fn update(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
    signals: DatastarSignals,
) -> Result<impl IntoResponse> {{
{signal_extractions}
    let mut v = Validator::new();
{validator_calls}    v.check()?;

    let item = {pascal}::update(&ctx.db, id{create_args}).await?;
    let html = {pascal}Show {{ item }}
        .render()
        .map_err(|e| Error::Internal(e.to_string()))?;
    SseResponse::new().patch_html(&html)
}}

pub async fn destroy(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
    pagination: PaginationParams,
) -> Result<impl IntoResponse> {{
    {pascal}::delete(&ctx.db, id).await?;
    let page = fetch_page(&ctx.db, pagination.page()).await?;
    SseFragment::new({pascal}ListFragment {{ page }})
}}
"#
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

/// Writes the index HTML template for a controller.
fn write_index_template(base: &Path, snake: &str, pascal: &str) -> Result<(), String> {
    let dir = base.join(format!("templates/pages/{snake}"));
    let path = dir.join("index.html");
    let content = format!(
        r#"{{% extends "layouts/app.html" %}}
{{% block title %}}{pascal} List{{% endblock %}}
{{% block content %}}
<h1>{pascal} List</h1>
{{% endblock %}}
"#
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

/// Writes the show HTML template for a controller.
fn write_show_template(base: &Path, snake: &str, pascal: &str) -> Result<(), String> {
    let dir = base.join(format!("templates/pages/{snake}"));
    let path = dir.join("show.html");
    let content = format!(
        r#"{{% extends "layouts/app.html" %}}
{{% block title %}}{pascal} Detail{{% endblock %}}
{{% block content %}}
<h1>{pascal} #{{{{ id }}}}</h1>
{{% endblock %}}
"#
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

/// Writes the index template for a scaffold (field-aware variant).
fn write_scaffold_index_template(
    base: &Path,
    snake: &str,
    pascal: &str,
    _fields: &[FieldDef],
) -> Result<(), String> {
    write_index_template(base, snake, pascal)
}

/// Writes the show template for a scaffold (field-aware variant).
fn write_scaffold_show_template(
    base: &Path,
    snake: &str,
    pascal: &str,
    _fields: &[FieldDef],
) -> Result<(), String> {
    write_show_template(base, snake, pascal)
}

/// Writes the model Rust source file with SQLx derives and CRUD methods.
fn write_model_file(
    base: &Path,
    snake: &str,
    pascal: &str,
    fields: &[FieldDef],
) -> Result<(), String> {
    let dir = base.join("src/models");
    let path = dir.join(format!("{snake}.rs"));
    let plural = format!("{snake}s");

    let struct_fields: String = fields
        .iter()
        .map(|f| format!("    pub {}: {},\n", f.name, f.rust_type()))
        .collect();

    let all_columns = build_column_list(fields);

    let select_cols = all_columns.join(", ");

    let mut methods = String::new();

    // find_by_id
    methods.push_str(&format!(
        r#"    pub async fn find_by_id(pool: &DbPool, id: i64) -> Result<Self> {{
        Ok(query_as!(Self, "SELECT {select_cols} FROM {plural} WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?)
    }}"#
    ));

    // find_all
    methods.push_str(&format!(
        r#"

    pub async fn find_all(pool: &DbPool) -> Result<Vec<Self>> {{
        Ok(query_as!(Self, "SELECT {select_cols} FROM {plural} ORDER BY id DESC")
            .fetch_all(pool)
            .await?)
    }}"#
    ));

    // create + update only when fields are present
    if !fields.is_empty() {
        let user_col_names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        let insert_cols = user_col_names.join(", ");
        let insert_placeholders: Vec<&str> = fields.iter().map(|_| "?").collect();
        let insert_placeholders = insert_placeholders.join(", ");

        let create_params: String = fields
            .iter()
            .map(|f| {
                if f.is_string() {
                    format!(", {}: &str", f.name)
                } else {
                    format!(", {}: {}", f.name, f.rust_type())
                }
            })
            .collect();

        let create_binds: String = fields
            .iter()
            .map(|f| format!("\n            .bind({})", f.name))
            .collect();

        methods.push_str(&format!(
            r#"

    pub async fn create(pool: &DbPool{create_params}) -> Result<Self> {{
        Ok(query_as!(Self, "INSERT INTO {plural} ({insert_cols}) VALUES ({insert_placeholders}) RETURNING {select_cols}"){create_binds}
            .fetch_one(pool)
            .await?)
    }}"#
        ));

        let set_clauses: Vec<String> = fields.iter().map(|f| format!("{} = ?", f.name)).collect();
        let set_clause = format!("{}, updated_at = CURRENT_TIMESTAMP", set_clauses.join(", "));

        let update_params: String = create_params.clone();
        let mut update_binds = create_binds.clone();
        update_binds.push_str("\n            .bind(id)");

        methods.push_str(&format!(
            r#"

    pub async fn update(pool: &DbPool, id: i64{update_params}) -> Result<Self> {{
        Ok(query_as!(Self, "UPDATE {plural} SET {set_clause} WHERE id = ? RETURNING {select_cols}"){update_binds}
            .fetch_one(pool)
            .await?)
    }}"#
        ));
    }

    // delete
    methods.push_str(&format!(
        r#"

    pub async fn delete(pool: &DbPool, id: i64) -> Result<()> {{
        query!("DELETE FROM {plural} WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }}"#
    ));

    let content = format!(
        r#"use blixt::prelude::*;
use sqlx::types::chrono::{{DateTime, Utc}};

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct {pascal} {{
    pub id: i64,
{struct_fields}    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}}

impl {pascal} {{
{methods}
}}
"#
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

fn build_column_list(fields: &[FieldDef]) -> Vec<String> {
    let mut cols = vec!["id".to_string()];
    for f in fields {
        cols.push(f.name.clone());
    }
    cols.push("created_at".to_string());
    cols.push("updated_at".to_string());
    cols
}

/// Writes a timestamped SQL migration file.
fn write_migration_file(
    base: &Path,
    snake: &str,
    plural: &str,
    fields: &[FieldDef],
) -> Result<(), String> {
    use crate::fields::{DbDialect, detect_dialect};

    let dialect = detect_dialect();
    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let dir = base.join("migrations");
    let path = dir.join(format!("{timestamp}_create_{snake}s.sql"));

    let (id_line, ts_type, ts_default) = match dialect {
        DbDialect::Postgres => ("id BIGSERIAL PRIMARY KEY", "TIMESTAMPTZ", "NOW()"),
        DbDialect::Sqlite => (
            "id INTEGER PRIMARY KEY AUTOINCREMENT",
            "DATETIME",
            "CURRENT_TIMESTAMP",
        ),
    };

    let field_lines: String = fields
        .iter()
        .map(|f| format!(",\n    {} {}", f.name, f.sql_type(dialect)))
        .collect();

    let content = format!(
        "CREATE TABLE IF NOT EXISTS {plural} (\
\n    {id_line}{field_lines},\
\n    created_at {ts_type} NOT NULL DEFAULT {ts_default},\
\n    updated_at {ts_type} NOT NULL DEFAULT {ts_default}\
\n);\n"
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

/// Writes a Datastar-ready list fragment template.
fn write_list_fragment(base: &Path, snake: &str, _fields: &[FieldDef]) -> Result<(), String> {
    let dir = base.join(format!("templates/fragments/{snake}"));
    let path = dir.join("list.html");
    let content = format!(
        r#"<div id="{snake}-list">
    {{% for item in items %}}
    <div>{{{{ item.id }}}}</div>
    {{% endfor %}}
</div>
"#
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

// --- Output helpers ---

/// Prints route registration hint after controller generation.
fn print_controller_route_hint(snake: &str) {
    println!("  {} controller {snake}", style("created").green().bold());
    println!(
        "\n  {} Add to src/main.rs routes:",
        style("next:").cyan().bold()
    );
    println!("    .route(\"/{snake}\", get(controllers::{snake}::index))");
    println!("    .route(\"/{snake}/{{id}}\", get(controllers::{snake}::show))");
}

/// Prints full CRUD route registration hints after scaffold generation.
fn print_scaffold_route_hints(snake: &str) {
    println!(
        "\n  {} Add CRUD routes to src/main.rs:",
        style("next:").cyan().bold()
    );
    println!("    .route(\"/{snake}\", get(controllers::{snake}::index))");
    println!("    .route(\"/{snake}\", post(controllers::{snake}::create))");
    println!("    .route(\"/{snake}/{{id}}\", get(controllers::{snake}::show))");
    println!("    .route(\"/{snake}/{{id}}\", put(controllers::{snake}::update))");
    println!("    .route(\"/{snake}/{{id}}\", delete(controllers::{snake}::destroy))");
}

// --- Filesystem utilities ---

/// Returns the current working directory as a `PathBuf`.
fn current_dir() -> Result<PathBuf, String> {
    std::env::current_dir().map_err(|err| format!("Failed to determine current directory: {err}"))
}

/// Creates a directory and all parents, returning an error on failure.
fn ensure_dir_exists(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|err| format!("Failed to create directory '{}': {err}", dir.display()))
}

/// Writes content to a file, failing if the file already exists.
fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if path.exists() {
        return Err(format!("File already exists: {}", path.display()));
    }
    fs::write(path, content).map_err(|err| format!("Failed to write '{}': {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn controller_creates_files_with_expected_content() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();

        generate_controller_in(base, "blog_post").expect("generate_controller_in failed");

        let controller = fs::read_to_string(base.join("src/controllers/blog_post.rs"))
            .expect("controller file missing");
        assert!(controller.contains("pub struct BlogPostIndex"));
        assert!(controller.contains("pub struct BlogPostShow"));
        assert!(controller.contains("pub async fn index()"));
        assert!(controller.contains("pub async fn show("));

        let index = fs::read_to_string(base.join("templates/pages/blog_post/index.html"))
            .expect("index template missing");
        assert!(index.contains("BlogPost List"));
        assert!(index.contains("extends \"layouts/app.html\""));

        let show = fs::read_to_string(base.join("templates/pages/blog_post/show.html"))
            .expect("show template missing");
        assert!(show.contains("BlogPost Detail"));
        assert!(show.contains("{{ id }}"));
    }

    #[test]
    fn model_creates_files_with_valid_structure() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();

        generate_model_in(base, "User", &[]).expect("generate_model_in failed");

        let model =
            fs::read_to_string(base.join("src/models/user.rs")).expect("model file missing");
        assert!(model.contains("pub struct User"));
        assert!(model.contains("pub id: i64"));
        assert!(model.contains("DateTime<Utc>"));
        assert!(model.contains("FromRow"));
        assert!(model.contains("find_by_id"));
        assert!(model.contains("find_all"));
        assert!(model.contains("delete"));
        assert!(model.contains("query_as!"));
        assert!(model.contains("query!"));

        let entries: Vec<_> = fs::read_dir(base.join("migrations"))
            .expect("migrations dir missing")
            .filter_map(|entry| entry.ok())
            .collect();
        assert_eq!(entries.len(), 1);

        let migration_path = entries[0].path();
        let filename = migration_path
            .file_name()
            .expect("no filename")
            .to_string_lossy();
        assert!(filename.ends_with("_create_users.sql"));

        let sql = fs::read_to_string(&migration_path).expect("migration file missing");
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS users"));
        assert!(sql.contains("BIGSERIAL PRIMARY KEY"));
        assert!(sql.contains("created_at TIMESTAMPTZ"));
        assert!(sql.contains("updated_at TIMESTAMPTZ"));
    }

    #[test]
    fn scaffold_creates_controller_model_and_fragment() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();

        let default_fields = vec![FieldDef {
            name: "name".into(),
            field_type: FieldType::String,
        }];
        generate_scaffold_in(base, "Product", &default_fields)
            .expect("generate_scaffold_in failed");

        assert!(base.join("src/controllers/product.rs").exists());
        assert!(base.join("src/models/product.rs").exists());
        assert!(base.join("templates/pages/product/index.html").exists());
        assert!(base.join("templates/pages/product/show.html").exists());

        let controller = fs::read_to_string(base.join("src/controllers/product.rs"))
            .expect("scaffold controller missing");
        assert!(controller.contains("Paginated"));
        assert!(controller.contains("find_by_id"));
        assert!(controller.contains("State(ctx)"));

        let fragment = fs::read_to_string(base.join("templates/fragments/product/list.html"))
            .expect("list fragment missing");
        assert!(fragment.contains("product-list"));
        assert!(fragment.contains("item.id"));

        let entries: Vec<_> = fs::read_dir(base.join("migrations"))
            .expect("migrations dir missing")
            .filter_map(|entry| entry.ok())
            .collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn model_with_fields_generates_struct_and_methods() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();
        let fields = vec![
            FieldDef {
                name: "title".into(),
                field_type: FieldType::String,
            },
            FieldDef {
                name: "published".into(),
                field_type: FieldType::Bool,
            },
        ];

        generate_model_in(base, "Article", &fields).expect("generate_model_in failed");

        let model =
            fs::read_to_string(base.join("src/models/article.rs")).expect("model file missing");
        assert!(model.contains("pub title: String"));
        assert!(model.contains("pub published: bool"));
        assert!(model.contains("pub async fn create("));
        assert!(model.contains("pub async fn update("));
        assert!(model.contains("INSERT INTO"));
        assert!(model.contains("UPDATE articles SET"));
        assert!(model.contains("RETURNING"));

        let migration_dir = base.join("migrations");
        let entries: Vec<_> = fs::read_dir(&migration_dir)
            .expect("migrations dir missing")
            .filter_map(|e| e.ok())
            .collect();
        let sql = fs::read_to_string(entries[0].path()).expect("migration file missing");
        assert!(sql.contains("title TEXT NOT NULL"));
        assert!(sql.contains("published BOOLEAN NOT NULL"));
    }

    #[test]
    fn scaffold_controller_has_all_crud_handlers() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();
        let fields = vec![FieldDef {
            name: "title".into(),
            field_type: FieldType::String,
        }];

        generate_scaffold_in(base, "Post", &fields).expect("scaffold failed");

        let controller =
            fs::read_to_string(base.join("src/controllers/post.rs")).expect("controller missing");

        assert!(controller.contains("pub async fn index("));
        assert!(controller.contains("pub async fn show("));
        assert!(controller.contains("pub async fn create("));
        assert!(controller.contains("pub async fn update("));
        assert!(controller.contains("pub async fn destroy("));

        assert!(controller.contains("PaginationParams"));
        assert!(controller.contains("Paginated"));
        assert!(controller.contains("DatastarSignals"));
        assert!(controller.contains("Validator"));
        assert!(controller.contains("SseResponse"));
        assert!(controller.contains("SseFragment"));
    }

    #[test]
    fn duplicate_generation_returns_error() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();

        generate_controller_in(base, "Item").expect("first generation failed");

        let result = generate_controller_in(base, "Item");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }
}
