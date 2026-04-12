//! Persistent job queue backed by Postgres or SQLite.
//!
//! Jobs survive process restarts, retry with exponential backoff on failure,
//! and are executed by a `Worker` that polls the database. On Postgres,
//! `LISTEN`/`NOTIFY` provides near-instant job pickup.

pub(crate) mod migration;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde_json::Value;
use tracing::{error, info, warn};

use crate::db::DbPool;
use crate::error::{Error, Result};

// --- Queue (enqueue API) ---

/// Enqueues jobs into the persistent queue.
pub struct Queue;

impl Queue {
    /// Start building a job to enqueue.
    pub fn enqueue<'a>(pool: &'a DbPool, job_type: &str, payload: Value) -> JobBuilder<'a> {
        JobBuilder {
            pool,
            job_type: job_type.to_owned(),
            payload,
            queue: "default".to_owned(),
            max_attempts: 5,
            delay: None,
        }
    }
}

/// Builder for configuring a job before enqueuing.
pub struct JobBuilder<'a> {
    pool: &'a DbPool,
    job_type: String,
    payload: Value,
    queue: String,
    max_attempts: i32,
    delay: Option<Duration>,
}

impl<'a> JobBuilder<'a> {
    /// Set the queue name (default: `"default"`).
    pub fn queue(mut self, queue: &str) -> Self {
        self.queue = queue.to_owned();
        self
    }

    /// Set the maximum number of retry attempts (default: 5).
    pub fn max_attempts(mut self, n: i32) -> Self {
        self.max_attempts = n;
        self
    }

    /// Delay execution by the given duration.
    pub fn delay(mut self, d: Duration) -> Self {
        self.delay = Some(d);
        self
    }

    /// Enqueue the job.
    pub async fn run(self) -> Result<i64> {
        let payload_str = serde_json::to_string(&self.payload)
            .map_err(|e| Error::Internal(format!("job payload serialize: {e}")))?;

        let scheduled_at = match self.delay {
            Some(d) => {
                let offset = chrono::Duration::seconds(d.as_secs() as i64);
                Utc::now() + offset
            }
            None => Utc::now(),
        };
        let scheduled_str = scheduled_at.to_rfc3339();

        let id = crate::db::builder::Insert::into("_blixt_jobs")
            .set("queue", &*self.queue)
            .set("job_type", &*self.job_type)
            .set("payload", &*payload_str)
            .set("max_attempts", self.max_attempts as i64)
            .set("scheduled_at", &*scheduled_str)
            .returning::<JobRow>(&["id"])
            .execute(self.pool)
            .await?;

        #[cfg(feature = "postgres")]
        notify_new_job(self.pool).await;

        info!(
            job_type = %self.job_type,
            queue = %self.queue,
            job_id = id.id,
            "job enqueued"
        );

        Ok(id.id)
    }
}

#[derive(sqlx::FromRow)]
struct JobRow {
    id: i64,
}

#[cfg(feature = "postgres")]
async fn notify_new_job(pool: &DbPool) {
    let _ = sqlx::query("SELECT pg_notify('_blixt_jobs', '')")
        .execute(pool)
        .await;
}

// --- Worker ---

type HandlerFn = Arc<
    dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync,
>;

/// Processes jobs from the persistent queue.
///
/// Register handlers for job types, then call `run()` to start polling.
pub struct Worker {
    pool: DbPool,
    queue: String,
    concurrency: usize,
    poll_interval: Duration,
    handlers: HashMap<String, HandlerFn>,
}

impl Worker {
    /// Create a worker with its own connection pool.
    pub fn new(pool: DbPool) -> Self {
        Self {
            pool,
            queue: "default".to_owned(),
            concurrency: 4,
            poll_interval: Duration::from_secs(5),
            handlers: HashMap::new(),
        }
    }

    /// Set the queue to process (default: `"default"`).
    pub fn queue(mut self, queue: &str) -> Self {
        self.queue = queue.to_owned();
        self
    }

    /// Set the maximum number of concurrent jobs (default: 4).
    pub fn concurrency(mut self, n: usize) -> Self {
        self.concurrency = n;
        self
    }

    /// Set the poll interval (default: 5 seconds).
    pub fn poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }

    /// Register a handler for a job type.
    pub fn register<F, Fut>(mut self, job_type: &str, handler: F) -> Self
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let handler: HandlerFn = Arc::new(move |payload| Box::pin(handler(payload)));
        self.handlers.insert(job_type.to_owned(), handler);
        self
    }

    /// Run the worker loop. Blocks until the process shuts down.
    pub async fn run(self) -> Result<()> {
        migration::ensure_jobs_table(&self.pool).await?;

        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.concurrency));
        let handlers = Arc::new(self.handlers);
        let pool = self.pool.clone();
        let queue = self.queue.clone();

        info!(
            queue = %queue,
            concurrency = self.concurrency,
            "job worker started"
        );

        // Postgres: spawn a listener for immediate wakeup
        #[cfg(feature = "postgres")]
        let notify = {
            let n = Arc::new(tokio::sync::Notify::new());
            let n2 = n.clone();
            let pool2 = pool.clone();
            tokio::spawn(async move {
                listen_for_jobs(&pool2, &n2).await;
            });
            n
        };

        loop {
            let jobs = fetch_pending_jobs(&pool, &queue, self.concurrency as i64).await?;

            if jobs.is_empty() {
                #[cfg(feature = "postgres")]
                {
                    tokio::select! {
                        _ = notify.notified() => {},
                        _ = tokio::time::sleep(self.poll_interval) => {},
                    }
                }
                #[cfg(not(feature = "postgres"))]
                {
                    tokio::time::sleep(self.poll_interval).await;
                }
                continue;
            }

            for job in jobs {
                let permit = semaphore.clone().acquire_owned().await
                    .map_err(|e| Error::Internal(format!("semaphore: {e}")))?;
                let pool = pool.clone();
                let handlers = handlers.clone();

                tokio::spawn(async move {
                    execute_job(&pool, &handlers, job).await;
                    drop(permit);
                });
            }
        }
    }
}

// --- Internal job execution ---

#[derive(sqlx::FromRow, Debug)]
struct PendingJob {
    id: i64,
    job_type: String,
    payload: String,
    attempts: i32,
    max_attempts: i32,
}

#[cfg(any(
    all(feature = "postgres", not(feature = "sqlite")),
    all(feature = "postgres", feature = "sqlite", docsrs),
))]
async fn fetch_pending_jobs(pool: &DbPool, queue: &str, limit: i64) -> Result<Vec<PendingJob>> {
    let now = Utc::now().to_rfc3339();
    let rows = sqlx::query_as::<_, PendingJob>(
        "UPDATE _blixt_jobs SET status = 'running', locked_at = NOW(), attempts = attempts + 1 \
         WHERE id IN ( \
             SELECT id FROM _blixt_jobs \
             WHERE queue = $1 AND status = 'pending' AND scheduled_at <= $2 \
             ORDER BY scheduled_at ASC \
             LIMIT $3 \
             FOR UPDATE SKIP LOCKED \
         ) \
         RETURNING id, job_type, payload, attempts, max_attempts"
    )
    .bind(queue)
    .bind(&now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Internal(format!("fetch jobs: {e}")))?;
    Ok(rows)
}

#[cfg(all(feature = "sqlite", not(feature = "postgres"), not(docsrs)))]
async fn fetch_pending_jobs(pool: &DbPool, queue: &str, limit: i64) -> Result<Vec<PendingJob>> {
    let now = Utc::now().to_rfc3339();
    // SQLite doesn't support FOR UPDATE SKIP LOCKED — use a two-step approach
    let rows = sqlx::query_as::<_, PendingJob>(
        "UPDATE _blixt_jobs SET status = 'running', locked_at = CURRENT_TIMESTAMP, attempts = attempts + 1 \
         WHERE id IN ( \
             SELECT id FROM _blixt_jobs \
             WHERE queue = ? AND status = 'pending' AND scheduled_at <= ? \
             ORDER BY scheduled_at ASC \
             LIMIT ? \
         ) \
         RETURNING id, job_type, payload, attempts, max_attempts"
    )
    .bind(queue)
    .bind(&now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Internal(format!("fetch jobs: {e}")))?;
    Ok(rows)
}

async fn execute_job(pool: &DbPool, handlers: &HashMap<String, HandlerFn>, job: PendingJob) {
    let Some(handler) = handlers.get(&job.job_type) else {
        warn!(job_type = %job.job_type, job_id = job.id, "unknown job type, marking dead");
        let _ = mark_dead(pool, job.id, "unknown job type").await;
        return;
    };

    let payload: Value = match serde_json::from_str(&job.payload) {
        Ok(v) => v,
        Err(e) => {
            error!(job_id = job.id, error = %e, "invalid job payload");
            let _ = mark_dead(pool, job.id, &format!("invalid payload: {e}")).await;
            return;
        }
    };

    info!(job_type = %job.job_type, job_id = job.id, attempt = job.attempts, "job started");

    match handler(payload).await {
        Ok(()) => {
            info!(job_type = %job.job_type, job_id = job.id, "job completed");
            let _ = mark_completed(pool, job.id).await;
        }
        Err(e) => {
            error!(job_type = %job.job_type, job_id = job.id, error = %e, "job failed");
            if job.attempts >= job.max_attempts {
                warn!(job_id = job.id, "max attempts reached, marking dead");
                let _ = mark_dead(pool, job.id, &e.to_string()).await;
            } else {
                let backoff_secs = 30 * (1i64 << (job.attempts - 1).min(10));
                let _ = mark_retry(pool, job.id, &e.to_string(), backoff_secs).await;
            }
        }
    }
}

async fn mark_completed(pool: &DbPool, id: i64) -> Result<()> {
    crate::db::builder::Update::table("_blixt_jobs")
        .set("status", "completed")
        .set_timestamp("completed_at")
        .where_eq("id", id)
        .execute_no_return(pool)
        .await
}

async fn mark_dead(pool: &DbPool, id: i64, error: &str) -> Result<()> {
    crate::db::builder::Update::table("_blixt_jobs")
        .set("status", "dead")
        .set("last_error", error)
        .where_eq("id", id)
        .execute_no_return(pool)
        .await
}

async fn mark_retry(pool: &DbPool, id: i64, error: &str, backoff_secs: i64) -> Result<()> {
    let next_run = Utc::now() + chrono::Duration::seconds(backoff_secs);
    crate::db::builder::Update::table("_blixt_jobs")
        .set("status", "pending")
        .set("last_error", error)
        .set("locked_at", crate::db::builder::Value::Null)
        .set("locked_by", crate::db::builder::Value::Null)
        .set("scheduled_at", &*next_run.to_rfc3339())
        .where_eq("id", id)
        .execute_no_return(pool)
        .await
}

#[cfg(feature = "postgres")]
async fn listen_for_jobs(pool: &DbPool, notify: &tokio::sync::Notify) {
    use sqlx::postgres::PgListener;
    let mut listener = match PgListener::connect_with(pool).await {
        Ok(l) => l,
        Err(e) => {
            error!(error = %e, "failed to create PG listener for jobs");
            return;
        }
    };
    if let Err(e) = listener.listen("_blixt_jobs").await {
        error!(error = %e, "failed to LISTEN on _blixt_jobs");
        return;
    }
    loop {
        match listener.recv().await {
            Ok(_) => notify.notify_one(),
            Err(e) => {
                error!(error = %e, "PG listener error, reconnecting");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
