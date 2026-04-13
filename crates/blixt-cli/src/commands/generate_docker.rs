use std::fs;
use std::path::Path;

use console::style;

use super::fs_utils::{current_dir, write_file};
use crate::tailwind::TAILWIND_VERSION;

pub fn generate_docker() -> Result<(), String> {
    let base = current_dir()?;
    generate_docker_in(&base)
}

fn generate_docker_in(base: &Path) -> Result<(), String> {
    let cargo_toml = fs::read_to_string(base.join("Cargo.toml"))
        .map_err(|_| "No Cargo.toml found. Run from a Blixt project directory.".to_string())?;
    let app_name = parse_package_name(&cargo_toml)?;
    let has_redis = detect_redis(base, &cargo_toml);

    println!();
    println!("  {} PostgreSQL database", style("detected").cyan().bold());
    if has_redis {
        println!("  {} Redis cache", style("detected").cyan().bold());
    }
    println!();

    write_file(&base.join("Dockerfile"), &dockerfile(&app_name))?;
    write_file(
        &base.join("docker-compose.yml"),
        &compose(&app_name, has_redis),
    )?;
    write_file(&base.join(".dockerignore"), &dockerignore())?;

    println!("  {} Dockerfile", style("created").green().bold());
    println!("  {} docker-compose.yml", style("created").green().bold());
    println!("  {} .dockerignore", style("created").green().bold());

    Ok(())
}

fn parse_package_name(cargo_toml: &str) -> Result<String, String> {
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name") && trimmed.contains('=') {
            if let Some(value) = trimmed.split('=').nth(1) {
                let name = value.trim().trim_matches('"').trim_matches('\'');
                if !name.is_empty() {
                    return Ok(name.to_string());
                }
            }
        }
    }
    Err("Could not find package name in Cargo.toml".to_string())
}

fn detect_redis(base: &Path, cargo_toml: &str) -> bool {
    if cargo_toml.contains("\"redis\"") {
        return true;
    }
    if let Ok(env_example) = fs::read_to_string(base.join(".env.example")) {
        if env_example.contains("REDIS_URL") {
            return true;
        }
    }
    false
}

fn dockerfile(app_name: &str) -> String {
    format!(
        r#"FROM rust:1-slim AS chef
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev curl && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .

RUN curl -sLo /usr/local/bin/tailwindcss \
    https://github.com/tailwindlabs/tailwindcss/releases/download/v{tailwind}/tailwindcss-linux-x64 \
    && chmod +x /usr/local/bin/tailwindcss
RUN tailwindcss --input static/css/app.css --output static/css/output.css --minify

RUN cargo build --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/{app_name} /app/server
COPY --from=builder /app/static /app/static
COPY --from=builder /app/migrations /app/migrations
ENV BLIXT_ENV=production
EXPOSE 3000
CMD ["/app/server"]
"#,
        tailwind = TAILWIND_VERSION,
        app_name = app_name,
    )
}

fn compose(app_name: &str, has_redis: bool) -> String {
    let db_name = app_name.replace('-', "_");
    let mut out = String::new();

    // app service
    out.push_str(&format!(
        r#"services:
  app:
    build: .
    ports:
      - "3000:3000"
    environment:
      HOST: "0.0.0.0"
      PORT: "3000"
      DATABASE_URL: "postgres://blixt:blixt@db:5432/{db_name}"
      BLIXT_ENV: "production"
"#
    ));
    if has_redis {
        out.push_str("      REDIS_URL: \"redis://redis:6379\"\n");
    }
    out.push_str(
        "    depends_on:\n\
         \x20     db:\n\
         \x20       condition: service_healthy\n",
    );
    if has_redis {
        out.push_str(
            "      redis:\n\
             \x20       condition: service_healthy\n",
        );
    }
    out.push_str(
        r#"    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/_ping"]
      interval: 10s
      timeout: 5s
      retries: 3
"#,
    );

    // postgres service
    out.push_str(&format!(
        r#"
  db:
    image: postgres:17
    environment:
      POSTGRES_USER: blixt
      POSTGRES_PASSWORD: blixt
      POSTGRES_DB: {db_name}
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U blixt"]
      interval: 5s
      timeout: 3s
      retries: 5
"#
    ));

    // redis service (conditional)
    if has_redis {
        out.push_str(
            r#"
  redis:
    image: redis:7
    volumes:
      - redis_data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 3s
      retries: 5
"#,
        );
    }

    // volumes
    out.push_str("\nvolumes:\n  postgres_data:\n");
    if has_redis {
        out.push_str("  redis_data:\n");
    }

    out
}

fn dockerignore() -> String {
    "target/\n\
     .git/\n\
     .env\n\
     .env.*\n\
     !.env.example\n\
     node_modules/\n\
     docs/\n\
     *.md\n"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_all_files() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"my-app\"\nversion = \"0.1.0\"\n",
        )
        .expect("write");

        generate_docker_in(tmp.path()).expect("generate");

        assert!(tmp.path().join("Dockerfile").exists());
        assert!(tmp.path().join("docker-compose.yml").exists());
        assert!(tmp.path().join(".dockerignore").exists());
    }

    #[test]
    fn dockerfile_contains_app_name() {
        let content = dockerfile("my-app");
        assert!(content.contains("target/release/my-app"));
        assert!(content.contains("cargo-chef"));
        assert!(content.contains("tailwindcss"));
    }

    #[test]
    fn compose_includes_postgres() {
        let content = compose("my-app", false);
        assert!(content.contains("postgres:17"));
        assert!(content.contains("POSTGRES_DB: my_app"));
        assert!(!content.contains("redis"));
    }

    #[test]
    fn compose_includes_redis_when_detected() {
        let content = compose("my-app", true);
        assert!(content.contains("redis:7"));
        assert!(content.contains("REDIS_URL"));
        assert!(content.contains("redis_data"));
    }

    #[test]
    fn compose_converts_hyphens_to_underscores_for_db() {
        let content = compose("my-cool-app", false);
        assert!(content.contains("POSTGRES_DB: my_cool_app"));
        assert!(content.contains("db:5432/my_cool_app"));
    }

    #[test]
    fn detects_redis_from_cargo_features() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let toml = r#"[dependencies]
blixt = { version = "0.5", features = ["redis"] }
"#;
        assert!(detect_redis(tmp.path(), toml));
    }

    #[test]
    fn detects_redis_from_env_example() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        fs::write(
            tmp.path().join(".env.example"),
            "DATABASE_URL=postgres://...\nREDIS_URL=redis://localhost:6379\n",
        )
        .expect("write");
        assert!(detect_redis(tmp.path(), "[package]\nname = \"x\"\n"));
    }

    #[test]
    fn no_redis_when_absent() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        assert!(!detect_redis(tmp.path(), "[package]\nname = \"x\"\n"));
    }

    #[test]
    fn fails_without_cargo_toml() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let result = generate_docker_in(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn fails_when_files_already_exist() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"my-app\"\n",
        )
        .expect("write");
        fs::write(tmp.path().join("Dockerfile"), "existing").expect("write");

        let result = generate_docker_in(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn dockerignore_excludes_target_and_env() {
        let content = dockerignore();
        assert!(content.contains("target/"));
        assert!(content.contains(".env"));
        assert!(content.contains("!.env.example"));
    }
}
