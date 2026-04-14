use std::path::Path;

use chrono::Utc;
use console::style;

use crate::fields::{DbDialect, FieldDef, FieldType, detect_dialect, parse_fields};
use crate::validate::{pluralize, to_pascal_case, to_snake_case};

use super::fs_utils::{current_dir, ensure_dir_exists, update_mod_file, write_file};

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
    update_mod_file(base, "controllers", &snake)?;
    write_index_template(base, &snake, &pascal)?;
    write_show_template(base, &snake, &pascal)?;

    print_controller_route_hint(&snake);
    Ok(())
}

/// Model generation rooted at `base`.
fn generate_model_in(base: &Path, name: &str, fields: &[FieldDef]) -> Result<(), String> {
    let snake = to_snake_case(name);
    let pascal = to_pascal_case(name);
    let plural = pluralize(&snake);

    write_model_file(base, &snake, &pascal, fields)?;
    update_mod_file(base, "models", &snake)?;
    write_migration_file(base, &snake, fields)?;

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
    update_mod_file(base, "controllers", &snake)?;
    write_scaffold_index_template(base, &snake, &pascal, fields)?;
    write_scaffold_show_template(base, &snake, &pascal, fields)?;
    write_form_fragment(base, &snake, &pascal, fields)?;
    write_list_fragment(base, &snake, fields)?;
    write_item_fragment(base, &snake, fields)?;

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
    let plural = pluralize(snake);

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

    let signal_clear_keys: String = fields
        .iter()
        .map(|f| format!("\"{}\"", f.name))
        .collect::<Vec<_>>()
        .join(", ");

    let content = format!(
        r#"use blixt::prelude::*;
use blixt::validate::Validator;
use blixt::datastar::Signals;
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
    Paginated::<{pascal}>::query_builder()
        .columns(&[{col_list}])
        .from("{plural}")
        .order_by("id DESC")
        .paginate(pool, &PaginationParams::new(page_num, PER_PAGE))
        .await
}}

pub async fn index(
    State(ctx): State<AppContext>,
    pagination: PaginationParams,
) -> Result<impl IntoResponse> {{
    let page = fetch_page(&ctx.db, pagination.page()).await?;
    render!({pascal}Index {{ page }})
}}

pub async fn show(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse> {{
    let item = {pascal}::find_by_id(&ctx.db, id).await?;
    render!({pascal}Show {{ item }})
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
        .signals(&Signals::clear(&[{signal_clear_keys}]))
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
    Ok(SseResponse::new().patch_html(&html))
}}

pub async fn destroy(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
    pagination: PaginationParams,
) -> Result<impl IntoResponse> {{
    {pascal}::delete(&ctx.db, id).await?;
    let mut page = fetch_page(&ctx.db, pagination.page()).await?;
    if page.items.is_empty() && page.page > 1 {{
        page = fetch_page(&ctx.db, page.page - 1).await?;
    }}
    SseFragment::new({pascal}ListFragment {{ page }})
}}

pub async fn page_handler(
    State(ctx): State<AppContext>,
    pagination: PaginationParams,
) -> Result<impl IntoResponse> {{
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

/// Writes the index template for a scaffold with form and list includes.
fn write_scaffold_index_template(
    base: &Path,
    snake: &str,
    pascal: &str,
    _fields: &[FieldDef],
) -> Result<(), String> {
    let dir = base.join(format!("templates/pages/{snake}"));
    let path = dir.join("index.html");
    let content = format!(
        r##"{{% extends "layouts/app.html" %}}
{{% block title %}}{pascal} List{{% endblock %}}
{{% block content %}}
<main class="min-h-screen flex justify-center px-4 pt-16 pb-12 sm:pt-24">
  <div class="w-full max-w-lg">
    <h1 class="text-lg font-medium text-zinc-200 mb-6">{pascal}s</h1>

    {{% include "fragments/{snake}/form.html" %}}
    {{% include "fragments/{snake}/list.html" %}}
  </div>
</main>
{{% endblock %}}
"##
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

/// Writes the show template for a scaffold with edit form and delete button.
fn write_scaffold_show_template(
    base: &Path,
    snake: &str,
    pascal: &str,
    fields: &[FieldDef],
) -> Result<(), String> {
    let dir = base.join(format!("templates/pages/{snake}"));
    let path = dir.join("show.html");
    let plural = pluralize(snake);

    let signal_attrs: String = fields
        .iter()
        .map(|f| match f.field_type {
            FieldType::String => {
                format!(
                    "\n      data-signals-{}=\"'{{{{{{ item.{} }}}}}}'\"",
                    f.name, f.name
                )
            }
            _ => {
                format!(
                    "\n      data-signals-{}=\"{{{{{{ item.{} }}}}}}\"",
                    f.name, f.name
                )
            }
        })
        .collect();

    let input_fields: String = fields
        .iter()
        .map(|f| {
            let label = capitalize_field_name(&f.name);
            match f.field_type {
                FieldType::Bool => format!(
                    r#"
      <label class="flex items-center gap-2 text-[13px] text-zinc-400">
        <input type="checkbox" data-bind:{name}
               class="rounded border-zinc-700 bg-zinc-900/40">
        {label}
      </label>"#,
                    name = f.name
                ),
                FieldType::Int | FieldType::Float => format!(
                    r#"
      <input
        type="number"
        data-bind:{name}
        placeholder="{label}"
        autocomplete="off"
        class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
               focus:outline-none focus:border-zinc-700 transition-colors"
      >"#,
                    name = f.name
                ),
                FieldType::String => format!(
                    r#"
      <input
        type="text"
        data-bind:{name}
        placeholder="{label}"
        autocomplete="off"
        class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
               focus:outline-none focus:border-zinc-700 transition-colors"
      >"#,
                    name = f.name
                ),
            }
        })
        .collect();

    let content = format!(
        r##"{{% extends "layouts/app.html" %}}
{{% block title %}}{pascal} #{{{{{{ item.id }}}}}}{{% endblock %}}
{{% block content %}}
<main class="min-h-screen flex justify-center px-4 pt-16 pb-12 sm:pt-24">
  <div class="w-full max-w-lg">
    <a href="/{plural}" class="text-[12px] text-zinc-600 hover:text-zinc-400 transition-colors">&larr; Back to {plural}</a>

    <h1 class="text-lg font-medium text-zinc-200 mt-4 mb-6">{pascal} #{{{{{{ item.id }}}}}}</h1>

    <form
      class="space-y-3"{signal_attrs}
      data-on:submit="@put('/{plural}/{{{{{{ item.id }}}}}}')"
    >{input_fields}

      <div class="flex gap-2 pt-2">
        <button type="submit"
          class="flex-1 px-4 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
                 bg-zinc-900/40 text-zinc-400 hover:text-zinc-200 hover:border-zinc-700
                 transition-colors cursor-pointer"
        >Save</button>
        <button type="button"
          data-on:click="@delete('/{plural}/{{{{{{ item.id }}}}}}')"
          class="px-4 py-2.5 text-[13px] rounded-lg border border-red-900/40
                 bg-red-950/20 text-red-400/60 hover:text-red-400 hover:border-red-800/60
                 transition-colors cursor-pointer"
        >Delete</button>
      </div>
    </form>
  </div>
</main>
{{% endblock %}}
"##
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

/// Writes the model Rust source file with query builder CRUD methods.
fn write_model_file(
    base: &Path,
    snake: &str,
    pascal: &str,
    fields: &[FieldDef],
) -> Result<(), String> {
    let dir = base.join("src/models");
    let path = dir.join(format!("{snake}.rs"));
    let plural = pluralize(snake);

    let struct_fields: String = fields
        .iter()
        .map(|f| format!("    pub {}: {},\n", f.name, f.rust_type()))
        .collect();

    let all_columns = build_column_list(fields);
    let columns_literal: String = all_columns
        .iter()
        .map(|c| format!("\"{}\"", c))
        .collect::<Vec<_>>()
        .join(", ");

    let mut methods = String::new();

    // find_by_id
    methods.push_str(
        r#"    pub async fn find_by_id(pool: &DbPool, id: i64) -> Result<Self> {
        Select::from(TABLE).columns(COLUMNS).where_eq("id", id)
            .fetch_one::<Self>(pool).await
    }"#,
    );

    // find_all
    methods.push_str(
        r#"

    pub async fn find_all(pool: &DbPool) -> Result<Vec<Self>> {
        Select::from(TABLE).columns(COLUMNS).order_by("id", Order::Desc)
            .fetch_all::<Self>(pool).await
    }"#,
    );

    // create + update only when fields are present
    if !fields.is_empty() {
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

        let create_sets: String = fields
            .iter()
            .map(|f| format!("\n            .set(\"{name}\", {name})", name = f.name))
            .collect();

        methods.push_str(&format!(
            r#"

    pub async fn create(pool: &DbPool{create_params}) -> Result<Self> {{
        Insert::into(TABLE){create_sets}
            .returning::<Self>(COLUMNS)
            .execute(pool).await
    }}"#
        ));

        let update_params = create_params.clone();
        let update_sets: String = fields
            .iter()
            .map(|f| format!("\n            .set(\"{name}\", {name})", name = f.name))
            .collect();

        methods.push_str(&format!(
            r#"

    pub async fn update(pool: &DbPool, id: i64{update_params}) -> Result<Self> {{
        Update::table(TABLE){update_sets}
            .set_timestamp("updated_at")
            .where_eq("id", id)
            .returning::<Self>(COLUMNS)
            .execute(pool).await
    }}"#
        ));
    }

    // delete
    methods.push_str(
        r#"

    pub async fn delete(pool: &DbPool, id: i64) -> Result<()> {
        Delete::from(TABLE).where_eq("id", id).execute(pool).await
    }"#,
    );

    let relation_impls = build_relation_impls(pascal, fields);

    let content = format!(
        r#"use blixt::prelude::*;
use sqlx::types::chrono::{{DateTime, Utc}};

const TABLE: &str = "{plural}";
const COLUMNS: &[&str] = &[{columns_literal}];

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct {pascal} {{
    pub id: i64,
{struct_fields}    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}}

impl HasId for {pascal} {{
    fn id(&self) -> i64 {{
        self.id
    }}
}}

impl {pascal} {{
{methods}
}}
{relation_impls}"#
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

fn build_relation_impls(pascal: &str, fields: &[FieldDef]) -> String {
    let mut out = String::new();
    for f in fields {
        if !f.is_foreign_key() {
            continue;
        }
        let parent_model = f.parent_model_name().unwrap();
        let parent_table = f.parent_table_name().unwrap();
        let fk = &f.name;

        out.push_str(&format!(
            r#"
impl BelongsTo<super::{parent_snake}::{parent_model}> for {pascal} {{
    const FOREIGN_KEY: &'static str = "{fk}";
    const PARENT_TABLE: &'static str = "{parent_table}";
    fn fk_value(&self) -> i64 {{
        self.{fk}
    }}
}}

impl ForeignKey<super::{parent_snake}::{parent_model}> for {pascal} {{
    fn fk_value(&self) -> i64 {{
        self.{fk}
    }}
}}
"#,
            parent_snake = fk.strip_suffix("_id").unwrap(),
        ));
    }
    out
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
fn write_migration_file(base: &Path, snake: &str, fields: &[FieldDef]) -> Result<(), String> {
    let dialect = detect_dialect();
    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let dir = base.join("migrations");
    let plural = pluralize(snake);
    let path = dir.join(format!("{timestamp}_create_{plural}.sql"));

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

/// Writes the create form fragment with Datastar signal bindings.
fn write_form_fragment(
    base: &Path,
    snake: &str,
    pascal: &str,
    fields: &[FieldDef],
) -> Result<(), String> {
    let dir = base.join(format!("templates/fragments/{snake}"));
    let path = dir.join("form.html");
    let plural = pluralize(snake);

    let signal_attrs: String = fields
        .iter()
        .map(|f| match f.field_type {
            FieldType::String => format!("\n  data-signals-{}=\"''\"", f.name),
            FieldType::Bool => format!("\n  data-signals-{}=\"false\"", f.name),
            FieldType::Int => format!("\n  data-signals-{}=\"0\"", f.name),
            FieldType::Float => format!("\n  data-signals-{}=\"0\"", f.name),
        })
        .collect();

    let input_fields: String = fields
        .iter()
        .map(|f| {
            let label = capitalize_field_name(&f.name);
            match f.field_type {
                FieldType::Bool => format!(
                    r#"
  <label class="flex items-center gap-2 text-[13px] text-zinc-400">
    <input type="checkbox" data-bind:{name}
           class="rounded border-zinc-700 bg-zinc-900/40">
    {label}
  </label>"#,
                    name = f.name
                ),
                FieldType::Int | FieldType::Float => format!(
                    r#"
  <input
    type="number"
    data-bind:{name}
    placeholder="{label}"
    autocomplete="off"
    class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
           bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
           focus:outline-none focus:border-zinc-700 transition-colors"
  >"#,
                    name = f.name
                ),
                FieldType::String => format!(
                    r#"
  <input
    type="text"
    data-bind:{name}
    placeholder="{label}"
    autocomplete="off"
    class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
           bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
           focus:outline-none focus:border-zinc-700 transition-colors"
  >"#,
                    name = f.name
                ),
            }
        })
        .collect();

    let content = format!(
        r##"<form
  class="mb-6 space-y-3"{signal_attrs}
  data-on:submit="@post('/{plural}')"
>{input_fields}

  <button type="submit"
    class="w-full px-4 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
           bg-zinc-900/40 text-zinc-400 hover:text-zinc-200 hover:border-zinc-700
           transition-colors cursor-pointer"
  >Create {pascal}</button>
</form>
"##
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

/// Writes a Datastar-ready list fragment with pagination.
fn write_list_fragment(base: &Path, snake: &str, _fields: &[FieldDef]) -> Result<(), String> {
    let dir = base.join(format!("templates/fragments/{snake}"));
    let path = dir.join("list.html");
    let plural = pluralize(snake);

    let content = format!(
        r##"<div id="{snake}-list">
{{% if page.items.is_empty() %}}
  <div class="border border-zinc-800/80 rounded-lg bg-zinc-900/40 px-4 py-8">
    <p class="text-zinc-600 text-[13px] text-center">No {plural} yet.</p>
  </div>
{{% else %}}
  <div class="border border-zinc-800/80 rounded-lg bg-zinc-900/40 divide-y divide-zinc-800/60">
  {{% for item in page.items %}}
    {{% include "fragments/{snake}/item.html" %}}
  {{% endfor %}}
  </div>

  <div class="mt-4 flex items-center justify-between text-[12px] text-zinc-600">
    <span>{{{{{{ page.total }}}}}} {snake}{{% if page.total != 1 %}}s{{% endif %}}</span>
    <div class="flex items-center gap-2">
      {{% if page.has_prev %}}
      <button
        data-on:click="@get('/{plural}/page?page={{{{{{ page.page - 1 }}}}}}')"
        class="px-2 py-1 rounded border border-zinc-800/80 hover:border-zinc-700
               text-zinc-500 hover:text-zinc-300 transition-colors cursor-pointer"
      >&larr; Prev</button>
      {{% endif %}}
      <span class="text-zinc-500">{{{{{{ page.page }}}}}} / {{{{{{ page.total_pages }}}}}}</span>
      {{% if page.has_next %}}
      <button
        data-on:click="@get('/{plural}/page?page={{{{{{ page.page + 1 }}}}}}')"
        class="px-2 py-1 rounded border border-zinc-800/80 hover:border-zinc-700
               text-zinc-500 hover:text-zinc-300 transition-colors cursor-pointer"
      >Next &rarr;</button>
      {{% endif %}}
    </div>
  </div>
{{% endif %}}
</div>
"##
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

/// Writes the item fragment for a single record in the list.
fn write_item_fragment(base: &Path, snake: &str, fields: &[FieldDef]) -> Result<(), String> {
    let dir = base.join(format!("templates/fragments/{snake}"));
    let path = dir.join("item.html");
    let plural = pluralize(snake);

    let display_field = fields
        .iter()
        .find(|f| f.is_string())
        .map(|f| f.name.as_str())
        .unwrap_or("id");

    let content = format!(
        r##"<div class="flex items-center gap-3 px-4 py-3 group">
  <a href="/{plural}/{{{{{{ item.id }}}}}}" class="flex-1 min-w-0">
    <span class="text-[13px] text-zinc-300 truncate block">
      {{{{{{ item.{display_field} }}}}}}
    </span>
  </a>
  <button
    data-on:click="@delete('/{plural}/{{{{{{ item.id }}}}}}?page={{{{{{ page.page }}}}}}')"
    class="shrink-0 opacity-0 group-hover:opacity-100 text-zinc-600 hover:text-red-400
           transition-all cursor-pointer p-0.5"
  >
    <svg class="size-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
      <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
    </svg>
  </button>
</div>
"##
    );

    ensure_dir_exists(&dir)?;
    write_file(&path, &content)
}

/// Capitalizes the first character and replaces underscores with spaces.
fn capitalize_field_name(name: &str) -> String {
    let mut s = name.replace('_', " ");
    if let Some(first) = s.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    s
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
    let plural = pluralize(snake);
    println!(
        "\n  {} Add CRUD routes to src/main.rs:",
        style("next:").cyan().bold()
    );
    println!("    .route(\"/{plural}\", get(controllers::{snake}::index))");
    println!("    .route(\"/{plural}\", post(controllers::{snake}::create))");
    println!("    .route(\"/{plural}/page\", get(controllers::{snake}::page_handler))");
    println!("    .route(\"/{plural}/{{id}}\", get(controllers::{snake}::show))");
    println!("    .route(\"/{plural}/{{id}}\", put(controllers::{snake}::update))");
    println!("    .route(\"/{plural}/{{id}}\", delete(controllers::{snake}::destroy))");
}

#[cfg(test)]
mod tests {
    use std::fs;

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
        assert!(model.contains("Select::from(TABLE)"));
        assert!(model.contains("Delete::from(TABLE)"));

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
        assert!(fragment.contains("page.items"));

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
        assert!(model.contains("Insert::into(TABLE)"));
        assert!(model.contains("Update::table(TABLE)"));
        assert!(model.contains(".returning::<Self>(COLUMNS)"));

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

    #[test]
    fn scaffold_index_includes_form_and_list() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();
        let fields = vec![
            FieldDef {
                name: "title".into(),
                field_type: FieldType::String,
            },
            FieldDef {
                name: "active".into(),
                field_type: FieldType::Bool,
            },
        ];

        generate_scaffold_in(base, "Widget", &fields).expect("scaffold failed");

        let index = fs::read_to_string(base.join("templates/pages/widget/index.html"))
            .expect("index template missing");
        assert!(index.contains("include \"fragments/widget/form.html\""));
        assert!(index.contains("include \"fragments/widget/list.html\""));

        let form = fs::read_to_string(base.join("templates/fragments/widget/form.html"))
            .expect("form fragment missing");
        assert!(form.contains("data-bind:title"));
        assert!(form.contains("data-bind:active"));
        assert!(form.contains("@post('/widgets')"));
    }

    #[test]
    fn scaffold_list_and_item_fragments() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();
        let fields = vec![FieldDef {
            name: "title".into(),
            field_type: FieldType::String,
        }];

        generate_scaffold_in(base, "Task", &fields).expect("scaffold failed");

        let list = fs::read_to_string(base.join("templates/fragments/task/list.html"))
            .expect("list fragment missing");
        assert!(list.contains("task-list"));
        assert!(list.contains("include \"fragments/task/item.html\""));
        assert!(list.contains("page.has_prev"));
        assert!(list.contains("page.has_next"));

        let item = fs::read_to_string(base.join("templates/fragments/task/item.html"))
            .expect("item fragment missing");
        assert!(item.contains("item.title"));
        assert!(item.contains("@delete("));
    }

    #[test]
    fn scaffold_show_page_has_edit_form() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();
        let fields = vec![
            FieldDef {
                name: "title".into(),
                field_type: FieldType::String,
            },
            FieldDef {
                name: "count".into(),
                field_type: FieldType::Int,
            },
        ];

        generate_scaffold_in(base, "Product", &fields).expect("scaffold failed");

        let show = fs::read_to_string(base.join("templates/pages/product/show.html"))
            .expect("show template missing");
        assert!(show.contains("extends \"layouts/app.html\""));
        assert!(show.contains("data-signals"));
        assert!(show.contains("item.title"));
        assert!(show.contains("item.count"));
        assert!(show.contains("@put("));
        assert!(show.contains("@delete("));
        assert!(show.contains("href=\"/products\""));
    }

    #[test]
    fn model_generation_creates_mod_file() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();

        generate_model_in(base, "User", &[]).expect("generate failed");

        let mod_rs = fs::read_to_string(base.join("src/models/mod.rs")).expect("mod.rs missing");
        assert!(mod_rs.contains("pub mod user;"));
    }

    #[test]
    fn scaffold_creates_both_mod_files() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();
        let fields = vec![FieldDef {
            name: "name".into(),
            field_type: FieldType::String,
        }];

        generate_scaffold_in(base, "Widget", &fields).expect("scaffold failed");

        let models_mod =
            fs::read_to_string(base.join("src/models/mod.rs")).expect("models/mod.rs missing");
        assert!(models_mod.contains("pub mod widget;"));

        let controllers_mod = fs::read_to_string(base.join("src/controllers/mod.rs"))
            .expect("controllers/mod.rs missing");
        assert!(controllers_mod.contains("pub mod widget;"));
    }

    #[test]
    fn mod_file_is_idempotent() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();

        let dir = base.join("src/models");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("mod.rs"), "pub mod existing;\n").unwrap();

        generate_model_in(base, "User", &[]).expect("generate failed");

        let mod_rs = fs::read_to_string(base.join("src/models/mod.rs")).expect("mod.rs missing");
        assert!(mod_rs.contains("pub mod existing;"));
        assert!(mod_rs.contains("pub mod user;"));
    }

    #[test]
    fn model_uses_query_builder() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let base = tmp.path();
        let fields = vec![
            FieldDef {
                name: "title".into(),
                field_type: FieldType::String,
            },
            FieldDef {
                name: "active".into(),
                field_type: FieldType::Bool,
            },
        ];

        generate_model_in(base, "Post", &fields).expect("generate failed");

        let model = fs::read_to_string(base.join("src/models/post.rs")).expect("model missing");
        assert!(model.contains("const TABLE: &str = \"posts\""));
        assert!(model.contains("const COLUMNS: &[&str]"));
        assert!(model.contains("Select::from(TABLE)"));
        assert!(model.contains("Insert::into(TABLE)"));
        assert!(model.contains("Update::table(TABLE)"));
        assert!(model.contains("Delete::from(TABLE)"));
        assert!(model.contains(".where_eq(\"id\", id)"));
        assert!(model.contains(".set(\"title\", title)"));
        assert!(model.contains(".set(\"active\", active)"));
        assert!(model.contains(".set_timestamp(\"updated_at\")"));
        assert!(!model.contains("$1"));
        assert!(!model.contains("query_as!"));
        assert!(!model.contains("query!"));
    }

    #[test]
    fn model_generates_has_id() {
        let tmp = TempDir::new().expect("temp dir");
        generate_model_in(tmp.path(), "Post", &[]).expect("generate failed");
        let model =
            fs::read_to_string(tmp.path().join("src/models/post.rs")).expect("model missing");
        assert!(model.contains("impl HasId for Post"));
    }

    #[test]
    fn model_with_fk_generates_belongs_to() {
        let tmp = TempDir::new().expect("temp dir");
        let fields = vec![
            FieldDef {
                name: "title".into(),
                field_type: FieldType::String,
            },
            FieldDef {
                name: "author_id".into(),
                field_type: FieldType::Int,
            },
        ];

        generate_model_in(tmp.path(), "Post", &fields).expect("generate failed");
        let model =
            fs::read_to_string(tmp.path().join("src/models/post.rs")).expect("model missing");
        assert!(
            model.contains("impl BelongsTo<super::author::Author> for Post"),
            "missing BelongsTo impl:\n{model}"
        );
        assert!(model.contains("FOREIGN_KEY: &'static str = \"author_id\""));
        assert!(model.contains("PARENT_TABLE: &'static str = \"authors\""));
        assert!(model.contains("impl ForeignKey<super::author::Author> for Post"));
    }

    #[test]
    fn model_without_fk_has_no_relationship_impls() {
        let tmp = TempDir::new().expect("temp dir");
        let fields = vec![FieldDef {
            name: "title".into(),
            field_type: FieldType::String,
        }];

        generate_model_in(tmp.path(), "Post", &fields).expect("generate failed");
        let model =
            fs::read_to_string(tmp.path().join("src/models/post.rs")).expect("model missing");
        assert!(!model.contains("BelongsTo"));
        assert!(!model.contains("ForeignKey"));
    }
}
