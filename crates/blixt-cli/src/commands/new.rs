use std::fs;
use std::io::IsTerminal;
use std::path::Path;

use clap::ValueEnum;
use console::style;
use dialoguer::Select;

use crate::validate;

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum DbBackend {
    #[value(alias = "pg")]
    Postgres,
    Sqlite,
}

impl DbBackend {
    fn prompt() -> Result<Self, String> {
        if !std::io::stdin().is_terminal() {
            return Err("no TTY detected. Use --db postgres or --db sqlite.".into());
        }
        let options = &["PostgreSQL", "SQLite"];
        let selection = Select::new()
            .with_prompt("Select a database")
            .items(options)
            .default(0)
            .interact()
            .map_err(|e| format!("prompt failed: {e}"))?;
        match selection {
            0 => Ok(Self::Postgres),
            1 => Ok(Self::Sqlite),
            _ => unreachable!(),
        }
    }
}

fn resolve_db_backend(db_arg: Option<DbBackend>) -> Result<DbBackend, String> {
    match db_arg {
        Some(db) => Ok(db),
        None => DbBackend::prompt(),
    }
}

const DATASTAR_VERSION: &str = "v1.0.0-RC.8";
const DATASTAR_URL: &str =
    "https://raw.githubusercontent.com/starfederation/datastar/v1.0.0-RC.8/bundles/datastar.js";

/// Logo SVG embedded at compile time from the repo root logo.svg
const LOGO_SVG: &str = include_str!("../../logo.svg");

pub async fn run(name: &str, db_arg: Option<DbBackend>) -> Result<(), String> {
    let db = resolve_db_backend(db_arg)?;
    run_in(Path::new("."), name, db).await
}

pub async fn run_in(base_dir: &Path, name: &str, db: DbBackend) -> Result<(), String> {
    let project = base_dir.join(name);
    check_no_existing_dir(&project)?;
    create_directories(&project)?;
    let pascal = validate::to_pascal_case(name);
    write_all_files(&project, name, &pascal, db)?;
    download_datastar(&project).await?;
    compile_tailwind(&project).await?;
    print_success(name);
    Ok(())
}

#[cfg(test)]
pub fn run_in_sync(base_dir: &Path, name: &str, db: DbBackend) -> Result<(), String> {
    let project = base_dir.join(name);
    check_no_existing_dir(&project)?;
    create_directories(&project)?;
    let pascal = validate::to_pascal_case(name);
    write_all_files(&project, name, &pascal, db)?;
    Ok(())
}

fn check_no_existing_dir(project: &Path) -> Result<(), String> {
    if project.exists() {
        return Err(format!("Directory '{}' already exists", project.display()));
    }
    Ok(())
}

fn create_directories(project: &Path) -> Result<(), String> {
    let dirs = [
        "src/controllers",
        "templates/layouts",
        "templates/pages",
        "templates/fragments",
        "templates/components",
        "templates/emails",
        "static/css",
        "static/js",
        "migrations",
    ];
    for dir in dirs {
        fs::create_dir_all(project.join(dir))
            .map_err(|e| format!("Failed to create {dir}: {e}"))?;
    }
    Ok(())
}

fn write_all_files(project: &Path, name: &str, pascal: &str, db: DbBackend) -> Result<(), String> {
    write_cargo_toml(project, name, db)?;
    write_main_rs(project)?;
    write_controllers_mod(project)?;
    write_home_controller(project, name)?;
    write_api_controller(project)?;
    write_layout_template(project)?;
    write_home_template(project, pascal)?;
    write_fragment_templates(project)?;
    write_logo(project)?;
    write_static_files(project)?;
    write_gitkeep_files(project)?;
    write_env_example(project, db)?;
    write_gitignore(project)?;
    Ok(())
}

fn write_file(project: &Path, relative: &str, content: &str) -> Result<(), String> {
    let path = project.join(relative);
    fs::write(&path, content).map_err(|e| format!("Failed to write {relative}: {e}"))
}

// --- Downloads ---

async fn download_datastar(project: &Path) -> Result<(), String> {
    println!(
        "  {} Downloading Datastar {DATASTAR_VERSION}...",
        style("↓").dim()
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;
    let resp = client
        .get(DATASTAR_URL)
        .send()
        .await
        .map_err(|e| format!("Failed to download Datastar: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Datastar download failed: HTTP {}", resp.status()));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read Datastar response: {e}"))?;
    write_file(project, "static/js/datastar.js", &body)
}

async fn compile_tailwind(project: &Path) -> Result<(), String> {
    let binary = crate::tailwind::ensure_tailwind().await?;
    println!("  {} Compiling Tailwind CSS...", style("↓").dim());
    let input = project.join("static/css/app.css");
    let output = project.join("static/css/output.css");
    let status = tokio::process::Command::new(&binary)
        .arg("--input")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .arg("--minify")
        .status()
        .await
        .map_err(|e| format!("Failed to run Tailwind: {e}"))?;
    if !status.success() {
        return Err("Tailwind compilation failed".into());
    }
    Ok(())
}

// --- File generators ---

fn write_cargo_toml(project: &Path, name: &str, db: DbBackend) -> Result<(), String> {
    let blixt_dep = match db {
        DbBackend::Postgres => r#"blixt = { git = "https://github.com/ripkitten-co/blixt" }"#,
        DbBackend::Sqlite => {
            r#"blixt = { git = "https://github.com/ripkitten-co/blixt", default-features = false, features = ["sqlite"] }"#
        }
    };
    let content = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[dependencies]
askama = "0.15"
axum = {{ version = "0.8", features = ["macros"] }}
{blixt_dep}
chrono = {{ version = "0.4", features = ["serde"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
tokio = {{ version = "1", features = ["full"] }}
tracing = "0.1"
"#
    );
    write_file(project, "Cargo.toml", &content)
}

fn write_main_rs(project: &Path) -> Result<(), String> {
    let content = r#"use blixt::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let config = Config::from_env()?;
    let app = App::new(config)
        .router(routes())
        .static_dir("static");

    app.serve().await
}

fn routes() -> Router {
    Router::new()
        // Pages
        .route("/", get(controllers::home::index))
        // API (JSON)
        .route("/api/status", get(controllers::api::status))
        // SSE fragments (Datastar)
        .route("/fragments/time", get(controllers::api::time_fragment))
        .route("/fragments/status", get(controllers::api::status_fragment))
}

mod controllers;
"#;
    write_file(project, "src/main.rs", content)
}

fn write_controllers_mod(project: &Path) -> Result<(), String> {
    write_file(
        project,
        "src/controllers/mod.rs",
        "pub mod api;\npub mod home;\n",
    )
}

fn write_home_controller(project: &Path, name: &str) -> Result<(), String> {
    let content = format!(
        r#"use askama::Template;
use axum::response::{{Html, IntoResponse}};

#[derive(Template)]
#[template(path = "pages/home.html")]
pub struct HomePage {{
    pub name: String,
    pub port: u16,
    pub env: String,
}}

pub async fn index() -> impl IntoResponse {{
    let page = HomePage {{
        name: "{name}".to_string(),
        port: std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3000),
        env: std::env::var("BLIXT_ENV")
            .unwrap_or_else(|_| "development".into()),
    }};
    Html(page.render().unwrap_or_default())
}}
"#
    );
    write_file(project, "src/controllers/home.rs", &content)
}

fn write_api_controller(project: &Path) -> Result<(), String> {
    let content = r#"use askama::Template;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub uptime_secs: u64,
    pub timestamp: String,
}

static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn sse_fragments(html: &str) -> Response {
    let oneline = html.trim().lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("");
    let sse = format!("event: datastar-patch-elements\ndata: elements {oneline}\n\n");
    (
        [(header::CONTENT_TYPE, "text/event-stream"),
         (header::CACHE_CONTROL, "no-cache")],
        sse,
    ).into_response()
}

/// GET /api/status — raw JSON API
pub async fn status() -> impl IntoResponse {
    let start = START.get_or_init(std::time::Instant::now);
    axum::Json(StatusResponse {
        status: "ok".into(),
        uptime_secs: start.elapsed().as_secs(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// GET /fragments/time — SSE fragment for Datastar
#[derive(Template)]
#[template(path = "fragments/time.html")]
pub struct TimeFragment {
    pub time: String,
}

pub async fn time_fragment() -> Response {
    let html = TimeFragment {
        time: chrono::Utc::now().format("%H:%M:%S%.3f UTC").to_string(),
    }.render().unwrap_or_default();
    sse_fragments(&html)
}

/// GET /fragments/status — SSE fragment showing API data
#[derive(Template)]
#[template(path = "fragments/status.html")]
pub struct StatusFragment {
    pub status: String,
    pub uptime: u64,
    pub timestamp: String,
}

pub async fn status_fragment() -> Response {
    let start = START.get_or_init(std::time::Instant::now);
    let html = StatusFragment {
        status: "ok".into(),
        uptime: start.elapsed().as_secs(),
        timestamp: chrono::Utc::now().format("%H:%M:%S UTC").to_string(),
    }.render().unwrap_or_default();
    sse_fragments(&html)
}
"#;
    write_file(project, "src/controllers/api.rs", content)
}

fn write_layout_template(project: &Path) -> Result<(), String> {
    let content = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="description" content="{% block description %}A Blixt application{% endblock %}">
    <title>{% block title %}Blixt App{% endblock %}</title>
    <link rel="icon" href="/static/logo.svg" type="image/svg+xml">
    <link rel="stylesheet" href="/static/css/output.css" data-blixt-css>
</head>
<body class="bg-zinc-950 text-zinc-300 antialiased">
    {% block content %}{% endblock %}
    <script type="module" src="/static/js/datastar.js"></script>
</body>
</html>
"#;
    write_file(project, "templates/layouts/app.html", content)
}

fn write_home_template(project: &Path, pascal_name: &str) -> Result<(), String> {
    let content = format!(
        r##"
{{% extends "layouts/app.html" %}}
{{% block title %}}{pascal_name}{{% endblock %}}
{{% block description %}}A lightning-fast web application built with Blixt{{% endblock %}}
{{% block content %}}
<main class="min-h-screen flex flex-col items-center justify-center p-6 selection:bg-amber-400/20">
  <div class="w-full max-w-xl">

    <div class="flex flex-col items-center mb-12">
      <img src="/static/logo.svg" alt="Blixt" class="w-10 h-16 mb-5">
      <h1 class="text-[15px] font-medium tracking-[0.25em] uppercase text-zinc-300">blixt</h1>
      <div class="flex items-center gap-3 mt-3">
        <span class="size-1.5 rounded-full bg-emerald-500 animate-pulse" aria-hidden="true"></span>
        <span class="font-mono text-[11px] text-zinc-400">
          :{{{{ port }}}} &middot; {{{{ env }}}}
        </span>
      </div>
    </div>

    <div class="grid grid-cols-3 gap-3 mb-3">
      <a href="https://github.com/ripkitten-co/blixt" target="_blank"
         class="group border border-zinc-800/80 rounded-lg bg-zinc-900/40 p-4
                hover:border-zinc-700 hover:bg-zinc-900/70 transition-all">
        <p class="text-[11px] font-mono text-zinc-400 group-hover:text-zinc-300 transition-colors mb-1">01</p>
        <p class="text-sm font-medium text-zinc-300 group-hover:text-zinc-100 transition-colors">Docs</p>
        <p class="text-[11px] text-zinc-400 mt-1 leading-relaxed">Read the guide</p>
      </a>
      <div class="group border border-zinc-800/80 rounded-lg bg-zinc-900/40 p-4
                  hover:border-zinc-700 hover:bg-zinc-900/70 transition-all cursor-default">
        <p class="text-[11px] font-mono text-zinc-400 group-hover:text-zinc-300 transition-colors mb-1">02</p>
        <p class="text-sm font-medium text-zinc-300 group-hover:text-zinc-100 transition-colors">Generate</p>
        <p class="text-[11px] text-zinc-400 mt-1 leading-relaxed font-mono">blixt g scaffold</p>
      </div>
      <div class="group border border-zinc-800/80 rounded-lg bg-zinc-900/40 p-4
                  hover:border-zinc-700 hover:bg-zinc-900/70 transition-all cursor-default">
        <p class="text-[11px] font-mono text-zinc-400 group-hover:text-zinc-300 transition-colors mb-1">03</p>
        <p class="text-sm font-medium text-zinc-300 group-hover:text-zinc-100 transition-colors">Edit</p>
        <p class="text-[11px] text-zinc-400 mt-1 leading-relaxed">controllers/home.rs</p>
      </div>
    </div>

    <!-- Datastar: client-side signals -->
    <div class="border border-zinc-800/80 rounded-lg bg-zinc-900/40 p-5 mb-3" data-signals:count="0">
      <div class="flex items-center justify-between">
        <div>
          <p class="text-[10px] font-mono text-zinc-500 uppercase tracking-widest mb-1.5">Client signals</p>
          <div class="flex items-baseline gap-3">
            <p class="text-3xl font-mono text-amber-400 tabular-nums leading-none" data-text="$count">0</p>
            <p class="text-[11px] text-zinc-400 font-mono">reactive, no server</p>
          </div>
        </div>
        <div class="flex gap-1.5">
          <button data-on:click="$count--"
            class="size-8 rounded border border-zinc-800 bg-zinc-900 text-zinc-400
                   hover:border-zinc-600 hover:text-zinc-300 transition-colors
                   font-mono text-sm cursor-pointer grid place-items-center">&minus;</button>
          <button data-on:click="$count++"
            class="size-8 rounded border border-zinc-800 bg-zinc-900 text-zinc-400
                   hover:border-zinc-600 hover:text-zinc-300 transition-colors
                   font-mono text-sm cursor-pointer grid place-items-center">+</button>
        </div>
      </div>
    </div>

    <!-- Datastar: SSE server fragment -->
    <div class="border border-zinc-800/80 rounded-lg bg-zinc-900/40 p-5 mb-3">
      <p class="text-[10px] font-mono text-zinc-500 uppercase tracking-widest mb-2">SSE fragment</p>
      <div class="flex items-center justify-between">
        <div id="server-time" class="font-mono text-sm text-zinc-400">Click fetch to get server time</div>
        <button data-on:click="@get('/fragments/time')"
          class="px-3 py-1.5 rounded border border-zinc-800 bg-zinc-900 text-zinc-400
                 hover:border-zinc-600 hover:text-zinc-300 transition-colors
                 font-mono text-xs cursor-pointer">Fetch</button>
      </div>
      <p class="text-[11px] text-zinc-400 font-mono mt-2">Server renders HTML, Datastar patches it in via SSE</p>
    </div>

    <!-- JSON API via SSE -->
    <div class="border border-zinc-800/80 rounded-lg bg-zinc-900/40 p-5 mb-3">
      <p class="text-[10px] font-mono text-zinc-500 uppercase tracking-widest mb-2">API &rarr; SSE</p>
      <div class="flex items-center justify-between">
        <div id="api-result" class="font-mono text-xs text-zinc-400">GET /api/status</div>
        <button data-on:click="@get('/fragments/status')"
          class="px-3 py-1.5 rounded border border-zinc-800 bg-zinc-900 text-zinc-400
                 hover:border-zinc-600 hover:text-zinc-300 transition-colors
                 font-mono text-xs cursor-pointer">Fetch</button>
      </div>
      <p class="text-[11px] text-zinc-400 font-mono mt-2">Server queries data, renders HTML fragment, patches DOM</p>
    </div>

    <nav class="flex items-center justify-center gap-5 mt-8 font-mono text-[11px] text-zinc-400" aria-label="Resources">
      <a href="https://github.com/ripkitten-co/blixt" class="hover:text-zinc-200 transition-colors">GitHub</a>
      <a href="https://data-star.dev" class="hover:text-zinc-200 transition-colors">Datastar</a>
      <a href="https://tailwindcss.com" class="hover:text-zinc-200 transition-colors">Tailwind</a>
    </nav>

  </div>
</main>
{{% endblock %}}
"##
    );
    write_file(project, "templates/pages/home.html", &content)
}

fn write_fragment_templates(project: &Path) -> Result<(), String> {
    write_file(
        project,
        "templates/fragments/time.html",
        r#"<div id="server-time" class="font-mono text-sm text-amber-400">{{ time }}</div>"#,
    )?;
    write_file(
        project,
        "templates/fragments/status.html",
        r#"<div id="api-result" class="font-mono text-xs text-amber-400/80 leading-relaxed">{ "status": "{{ status }}", "uptime": {{ uptime }}, "time": "{{ timestamp }}" }</div>"#,
    )?;
    Ok(())
}

fn write_logo(project: &Path) -> Result<(), String> {
    write_file(project, "static/logo.svg", LOGO_SVG)
}

fn write_static_files(project: &Path) -> Result<(), String> {
    let css = r#"@import "tailwindcss";
@source "../../templates";
@source "../../src";
"#;
    write_file(project, "static/css/app.css", css)
}

fn write_gitkeep_files(project: &Path) -> Result<(), String> {
    let paths = [
        "migrations/.gitkeep",
        "templates/components/.gitkeep",
        "templates/emails/.gitkeep",
    ];
    for path in paths {
        write_file(project, path, "")?;
    }
    Ok(())
}

fn write_env_example(project: &Path, db: DbBackend) -> Result<(), String> {
    let db_url = match db {
        DbBackend::Postgres => "DATABASE_URL=postgres://localhost/my_app",
        DbBackend::Sqlite => "DATABASE_URL=sqlite://data.db",
    };
    let content = format!(
        "\
BLIXT_ENV=development
HOST=127.0.0.1
PORT=3000
{db_url}
JWT_SECRET=change-me-to-a-random-secret-at-least-32-chars
"
    );
    write_file(project, ".env.example", &content)
}

fn write_gitignore(project: &Path) -> Result<(), String> {
    let content = "\
/target/
.env
static/css/output.css
";
    write_file(project, ".gitignore", content)
}

fn print_success(name: &str) {
    println!(
        "\n  {} Created project {}",
        style("✓").green().bold(),
        style(name).cyan().bold()
    );
    println!(
        "\n  Get started:\n    {} {name}\n    {} dev\n",
        style("cd").white().bold(),
        style("blixt").white().bold()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_project_dir(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("blixt-test-new").join(format!(
            "{}_{}",
            suffix,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn scaffolds_all_expected_files() {
        let base = temp_project_dir("scaffold");
        let project_name = "test_app";
        let project_dir = base.join(project_name);

        let result = run_in_sync(&base, project_name, DbBackend::Postgres);
        assert!(result.is_ok(), "run_in_sync() failed: {:?}", result.err());

        let expected_files = [
            "Cargo.toml",
            "src/main.rs",
            "src/controllers/mod.rs",
            "src/controllers/home.rs",
            "src/controllers/api.rs",
            "templates/layouts/app.html",
            "templates/pages/home.html",
            "templates/fragments/time.html",
            "static/css/app.css",
            "migrations/.gitkeep",
            ".env.example",
            ".gitignore",
        ];

        for file in expected_files {
            let path = project_dir.join(file);
            assert!(path.exists(), "Missing expected file: {file}");
        }

        let layout = fs::read_to_string(project_dir.join("templates/layouts/app.html"))
            .expect("read layout");
        assert!(!layout.contains("cdn"), "Layout must not reference any CDN");

        let main = fs::read_to_string(project_dir.join("src/main.rs")).expect("read main.rs");
        assert!(
            main.contains("/api/status"),
            "main.rs should register API route"
        );
        assert!(
            main.contains("/fragments/time"),
            "main.rs should register SSE route"
        );

        let cargo_toml =
            fs::read_to_string(project_dir.join("Cargo.toml")).expect("read Cargo.toml");
        assert!(
            !cargo_toml.contains("default-features = false"),
            "postgres project should use default features"
        );

        let env_example =
            fs::read_to_string(project_dir.join(".env.example")).expect("read .env.example");
        assert!(
            env_example.contains("postgres://"),
            "should use postgres URL"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn rejects_existing_directory() {
        let base = temp_project_dir("existing");
        let project_name = "existing_proj";
        let project_dir = base.join(project_name);
        fs::create_dir_all(&project_dir).expect("create existing dir");

        let result = run_in_sync(&base, project_name, DbBackend::Postgres);

        assert!(result.is_err());
        assert!(
            result
                .err()
                .expect("should error")
                .contains("already exists")
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn scaffolds_sqlite_project() {
        let base = temp_project_dir("sqlite");
        let project_name = "sqlite_app";
        let project_dir = base.join(project_name);

        let result = run_in_sync(&base, project_name, DbBackend::Sqlite);
        assert!(result.is_ok(), "run_in_sync() failed: {:?}", result.err());

        let cargo_toml =
            fs::read_to_string(project_dir.join("Cargo.toml")).expect("read Cargo.toml");
        assert!(
            cargo_toml.contains(r#"features = ["sqlite"]"#),
            "should use sqlite feature"
        );
        assert!(
            !cargo_toml.contains(r#"features = ["postgres"]"#),
            "should not use postgres feature"
        );

        let env_example =
            fs::read_to_string(project_dir.join(".env.example")).expect("read .env.example");
        assert!(env_example.contains("sqlite://"), "should use sqlite URL");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn db_backend_value_enum_parses_valid_values() {
        use clap::ValueEnum;
        assert_eq!(
            DbBackend::from_str("postgres", true).unwrap(),
            DbBackend::Postgres
        );
        assert_eq!(
            DbBackend::from_str("pg", true).unwrap(),
            DbBackend::Postgres
        );
        assert_eq!(
            DbBackend::from_str("sqlite", true).unwrap(),
            DbBackend::Sqlite
        );
        // case insensitive
        assert_eq!(
            DbBackend::from_str("POSTGRES", true).unwrap(),
            DbBackend::Postgres
        );
    }

    #[test]
    fn db_backend_value_enum_rejects_invalid_values() {
        use clap::ValueEnum;
        assert!(DbBackend::from_str("mysql", true).is_err());
        assert!(DbBackend::from_str("", true).is_err());
    }
}
