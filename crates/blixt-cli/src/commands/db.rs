use std::path::Path;

use console::style;
use sqlx::PgPool;
use sqlx::migrate::Migrator;

/// Runs all pending migrations against the database.
pub async fn migrate() -> Result<(), String> {
    let pool = connect_to_database().await?;
    let migrator = load_migrator().await?;

    println!(
        "  {} Running pending migrations...",
        style("▸").cyan().bold()
    );

    let before = count_applied(&pool).await;
    migrator
        .run(&pool)
        .await
        .map_err(|err| format!("Migration failed: {err}"))?;
    let after = count_applied(&pool).await;

    let applied = after.saturating_sub(before);
    println!(
        "  {} Applied {applied} migration(s) successfully.",
        style("✓").green().bold()
    );

    pool.close().await;
    Ok(())
}

/// Reverts the most recently applied migration.
pub async fn rollback() -> Result<(), String> {
    let pool = connect_to_database().await?;
    let migrator = load_migrator().await?;

    println!(
        "  {} Rolling back last migration...",
        style("▸").cyan().bold()
    );

    migrator
        .undo(&pool, 1)
        .await
        .map_err(|err| format!("Rollback failed: {err}"))?;

    println!(
        "  {} Rolled back 1 migration successfully.",
        style("✓").green().bold()
    );

    pool.close().await;
    Ok(())
}

/// Prints a table showing applied and pending migrations.
pub async fn status() -> Result<(), String> {
    let pool = connect_to_database().await?;
    let migrator = load_migrator().await?;

    println!("  {} Migration status:", style("▸").cyan().bold());
    println!();

    print_migration_table(&pool, &migrator).await?;

    pool.close().await;
    Ok(())
}

/// Loads the .env file and reads DATABASE_URL from the environment.
fn read_database_url() -> Result<String, String> {
    dotenvy::dotenv().ok();
    std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL not set. Add it to .env or export it.".to_string())
}

/// Connects to the database using DATABASE_URL from the environment.
async fn connect_to_database() -> Result<PgPool, String> {
    let url = read_database_url()?;
    PgPool::connect(&url)
        .await
        .map_err(|err| format!("Failed to connect to database: {err}"))
}

/// Loads the runtime migrator from the `./migrations` directory.
async fn load_migrator() -> Result<Migrator, String> {
    let migrations_dir = Path::new("./migrations");
    if !migrations_dir.exists() {
        return Err(
            "No migrations directory found. Create one with `blixt generate model`.".to_string(),
        );
    }
    Migrator::new(migrations_dir)
        .await
        .map_err(|err| format!("Failed to load migrations: {err}"))
}

/// Counts the number of applied migrations by querying the database.
async fn count_applied(pool: &PgPool) -> u64 {
    let row: Option<(i64,)> = sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

    row.map(|(count,)| count as u64).unwrap_or(0)
}

/// Prints each migration with its applied/pending status.
async fn print_migration_table(pool: &PgPool, migrator: &Migrator) -> Result<(), String> {
    let applied = fetch_applied_versions(pool).await;

    for migration in migrator.iter() {
        let version = migration.version;
        let description = &migration.description;
        let is_applied = applied.contains(&version);

        let status_label = if is_applied {
            style("applied").green().to_string()
        } else {
            style("pending").yellow().to_string()
        };

        println!("    {version:>14} {status_label:>18}  {description}");
    }

    println!();
    Ok(())
}

/// Fetches the list of applied migration versions from the database.
async fn fetch_applied_versions(pool: &PgPool) -> Vec<i64> {
    let rows: Vec<(i64,)> = sqlx::query_as("SELECT version FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    rows.into_iter().map(|(version,)| version).collect()
}
