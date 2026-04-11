+++
title = "Background Jobs"
weight = 12
description = "Running async background work with bounded concurrency using JobRunner."
+++

# Background Jobs

Blixt includes a lightweight background job system built on Tokio. The `JobRunner` processes jobs via an internal bounded channel with configurable concurrency. Job failures are logged but never crash the runner.

## JobRunner

Create a runner with a concurrency limit:

```rust
use blixt::prelude::*;

// Run up to 4 jobs in parallel (the default)
let runner = JobRunner::default_runner();

// Or specify a custom concurrency limit
let runner = JobRunner::new(8);
```

The runner spawns a background Tokio task that pulls jobs from a bounded channel (capacity 100) and executes them using a semaphore to enforce the concurrency limit.

## The Job trait

Any type implementing the `Job` trait can be submitted to the runner:

```rust
use blixt::prelude::*;
use std::pin::Pin;
use std::future::Future;

struct SendWelcomeEmail {
    user_id: i64,
    email: String,
}

impl Job for SendWelcomeEmail {
    fn name(&self) -> &str {
        "send_welcome_email"
    }

    fn run(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async {
            // send the email...
            info!(user_id = self.user_id, "Welcome email sent");
            Ok(())
        })
    }
}
```

The `name()` method is used for structured logging. When a job starts, completes, or fails, Blixt logs the job name automatically.

## job_fn helper

For one-off jobs that don't need a dedicated struct, use `job_fn`:

```rust
use blixt::prelude::*;

let job = job_fn("cleanup_expired_sessions", || async {
    // ... cleanup logic ...
    Ok(())
});

runner.submit(job).await?;
```

`job_fn` takes a name and an async closure, returning an anonymous type that implements `Job`.

## Submitting jobs

Use `JobRunner::submit()` to enqueue a job. This is an async call that waits for channel capacity if the internal buffer (100 items) is full.

```rust
runner.submit(SendWelcomeEmail {
    user_id: 42,
    email: "user@example.com".into(),
}).await?;
```

If the runner's channel has been closed (e.g. the runner task was dropped), `submit` returns `Error::Internal("Job channel closed")`.

## Sharing the runner

Store the `JobRunner` in your application context so handlers can submit jobs:

```rust
use std::sync::Arc;

struct AppState {
    db: DbPool,
    jobs: Arc<JobRunner>,
}

pub async fn register_user(
    State(state): State<Arc<AppState>>,
    Form(input): Form<RegisterInput>,
) -> Result<impl IntoResponse> {
    let user = create_user(&state.db, &input).await?;

    let email = input.email.clone();
    state.jobs.submit(job_fn("welcome_email", move || {
        let email = email.clone();
        async move {
            // send welcome email to `email`
            Ok(())
        }
    })).await?;

    Ok(Html("registered"))
}
```

## Error handling

Job failures are logged at `error` level with the job name and error message, but they do not propagate to callers or crash the runner. The runner continues processing the next job in the queue.

```
INFO  job="send_welcome_email" Job started
ERROR job="send_welcome_email" error="SMTP send error: ..." Job failed
INFO  job="cleanup_sessions" Job started
INFO  job="cleanup_sessions" Job completed
```
