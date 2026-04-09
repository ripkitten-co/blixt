use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

/// A background job that can be submitted to the [`JobRunner`].
pub trait Job: Send + Sync + 'static {
    /// Unique name for this job type, used for logging.
    fn name(&self) -> &str;

    /// Execute the job. Returns a [`Result`](crate::error::Result) on completion.
    fn run(&self) -> Pin<Box<dyn Future<Output = crate::error::Result<()>> + Send + '_>>;
}

/// Processes background jobs via Tokio tasks with bounded concurrency.
pub struct JobRunner {
    sender: mpsc::Sender<Box<dyn Job>>,
}

impl JobRunner {
    /// Create a new `JobRunner` with the given concurrency limit.
    ///
    /// Spawns a background task that pulls jobs from a bounded channel
    /// and executes them with at most `concurrency` jobs running in
    /// parallel. Job failures are logged but never crash the runner.
    pub fn new(concurrency: usize) -> Self {
        let (sender, receiver) = mpsc::channel::<Box<dyn Job>>(100);
        tokio::spawn(run_worker_loop(receiver, concurrency));
        Self { sender }
    }

    /// Submit a job for background execution.
    pub async fn submit(&self, job: impl Job) -> crate::error::Result<()> {
        self.sender
            .send(Box::new(job))
            .await
            .map_err(|_| crate::error::Error::Internal("Job channel closed".into()))
    }

    /// Create a runner with a default concurrency of 4.
    pub fn default_runner() -> Self {
        Self::new(4)
    }
}

/// Internal worker loop that receives and executes jobs.
async fn run_worker_loop(mut receiver: mpsc::Receiver<Box<dyn Job>>, concurrency: usize) {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    while let Some(job) = receiver.recv().await {
        let permit = semaphore.clone().acquire_owned().await;
        tokio::spawn(async move {
            let name = job.name().to_string();
            info!(job = %name, "Job started");
            match job.run().await {
                Ok(()) => info!(job = %name, "Job completed"),
                Err(e) => error!(job = %name, error = %e, "Job failed"),
            }
            drop(permit);
        });
    }
}

/// Create a [`Job`] from an async closure.
///
/// This is a convenience helper for one-off jobs that don't need
/// a dedicated struct implementing the `Job` trait.
///
/// # Example
///
/// ```ignore
/// let job = job_fn("send_email", || async {
///     // ... send email logic ...
///     Ok(())
/// });
/// runner.submit(job).await?;
/// ```
pub fn job_fn<F, Fut>(name: impl Into<String>, f: F) -> impl Job
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = crate::error::Result<()>> + Send + 'static,
{
    FnJob {
        name: name.into(),
        f,
    }
}

struct FnJob<F> {
    name: String,
    f: F,
}

impl<F, Fut> Job for FnJob<F>
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = crate::error::Result<()>> + Send + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&self) -> Pin<Box<dyn Future<Output = crate::error::Result<()>> + Send + '_>> {
        Box::pin((self.f)())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn submitted_job_executes() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();

        let runner = JobRunner::new(1);
        let job = job_fn("test_job", move || {
            let flag = flag_clone.clone();
            async move {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            }
        });

        runner.submit(job).await.expect("submit should succeed");
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(flag.load(Ordering::SeqCst), "Job should have executed");
    }

    #[tokio::test]
    async fn runner_continues_after_job_failure() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let runner = JobRunner::new(1);

        let failing_job = job_fn("failing_job", || async {
            Err(crate::error::Error::Internal("boom".into()))
        });

        let success_job = job_fn("success_job", move || {
            let counter = counter_clone.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        });

        runner
            .submit(failing_job)
            .await
            .expect("submit failing job");
        tokio::time::sleep(Duration::from_millis(50)).await;

        runner
            .submit(success_job)
            .await
            .expect("submit success job");
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "Runner should continue processing after a failure"
        );
    }

    #[tokio::test]
    async fn job_fn_creates_valid_job() {
        let job = job_fn("my_job", || async { Ok(()) });

        assert_eq!(job.name(), "my_job");
        let result = job.run().await;
        assert!(result.is_ok());
    }
}
