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
    _fields: &[FieldDef],
) -> Result<(), String> {
    let dir = base.join("src/controllers");
    let path = dir.join(format!("{snake}.rs"));
    let content = format!(
        r#"use blixt::prelude::*;
use crate::models::{snake}::{pascal};

#[derive(Template)]
#[template(path = "pages/{snake}/index.html")]
pub struct {pascal}Index {{
    pub items: Vec<{pascal}>,
}}

pub async fn index(State(ctx): State<AppContext>) -> Result<impl IntoResponse> {{
    let items = {pascal}::find_all(&ctx.db).await?;
    Ok(Html({pascal}Index {{ items }}.render().map_err(|e| Error::Internal(e.to_string()))?))
}}

#[derive(Template)]
#[template(path = "pages/{snake}/show.html")]
pub struct {pascal}Show {{
    pub item: {pascal},
}}

pub async fn show(State(ctx): State<AppContext>, Path(id): Path<i64>) -> Result<impl IntoResponse> {{
    let item = {pascal}::find_by_id(&ctx.db, id).await?;
    Ok(Html({pascal}Show {{ item }}.render().map_err(|e| Error::Internal(e.to_string()))?))
}}

pub async fn destroy(State(ctx): State<AppContext>, Path(id): Path<i64>) -> Result<impl IntoResponse> {{
    {pascal}::delete(&ctx.db, id).await?;
    let items = {pascal}::find_all(&ctx.db).await?;
    Ok(SseFragment::new({pascal}Index {{ items }})?)
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
    _fields: &[FieldDef],
) -> Result<(), String> {
    let dir = base.join("src/models");
    let path = dir.join(format!("{snake}.rs"));
    let plural = format!("{snake}s");
    let content = format!(
        r#"use blixt::prelude::*;
use sqlx::types::chrono::{{DateTime, Utc}};

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct {pascal} {{
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}}

impl {pascal} {{
    /// Find a single record by primary key.
    pub async fn find_by_id(pool: &DbPool, id: i64) -> Result<Self> {{
        Ok(query_as!(Self, "SELECT id, created_at, updated_at FROM {plural} WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?)
    }}

    /// Fetch all records ordered by most recent first.
    pub async fn find_all(pool: &DbPool) -> Result<Vec<Self>> {{
        Ok(query_as!(Self, "SELECT id, created_at, updated_at FROM {plural} ORDER BY id DESC")
            .fetch_all(pool)
            .await?)
    }}

    /// Delete a record by primary key.
    pub async fn delete(pool: &DbPool, id: i64) -> Result<()> {{
        query!("DELETE FROM {plural} WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }}
}}
"#
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

/// Writes a timestamped SQL migration file.
fn write_migration_file(
    base: &Path,
    snake: &str,
    plural: &str,
    _fields: &[FieldDef],
) -> Result<(), String> {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let dir = base.join("migrations");
    let path = dir.join(format!("{timestamp}_create_{snake}s.sql"));
    let content = format!(
        r#"CREATE TABLE IF NOT EXISTS {plural} (
    id BIGSERIAL PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
"#
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
        assert!(controller.contains("find_all"));
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
    fn duplicate_generation_returns_error() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();

        generate_controller_in(base, "Item").expect("first generation failed");

        let result = generate_controller_in(base, "Item");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }
}
