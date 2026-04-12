use crate::db::DbPool;
use crate::error::Result;

#[cfg(any(
    all(feature = "postgres", not(feature = "sqlite")),
    all(feature = "postgres", feature = "sqlite", docsrs),
))]
const CREATE_JOBS_TABLE: &str = "\
CREATE TABLE IF NOT EXISTS _blixt_jobs (
    id BIGSERIAL PRIMARY KEY,
    queue TEXT NOT NULL DEFAULT 'default',
    job_type TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'pending',
    attempts INT NOT NULL DEFAULT 0,
    max_attempts INT NOT NULL DEFAULT 5,
    last_error TEXT,
    scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    locked_at TIMESTAMPTZ,
    locked_by TEXT,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_blixt_jobs_fetch
    ON _blixt_jobs (queue, status, scheduled_at)
    WHERE status = 'pending';
";

#[cfg(all(feature = "sqlite", not(feature = "postgres"), not(docsrs)))]
const CREATE_JOBS_TABLE: &str = "\
CREATE TABLE IF NOT EXISTS _blixt_jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    queue TEXT NOT NULL DEFAULT 'default',
    job_type TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    last_error TEXT,
    scheduled_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    locked_at DATETIME,
    locked_by TEXT,
    completed_at DATETIME,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_blixt_jobs_fetch
    ON _blixt_jobs (queue, status, scheduled_at);
";

/// Creates the `_blixt_jobs` table if it doesn't exist.
pub async fn ensure_jobs_table(pool: &DbPool) -> Result<()> {
    for statement in CREATE_JOBS_TABLE.split(";\n").filter(|s| !s.trim().is_empty()) {
        sqlx::query(statement)
            .execute(pool)
            .await
            .map_err(|e| crate::error::Error::Internal(format!("jobs migration: {e}")))?;
    }
    tracing::info!("Job queue table ready");
    Ok(())
}
