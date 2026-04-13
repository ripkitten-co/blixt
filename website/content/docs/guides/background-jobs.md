+++
title = "Background Jobs"
weight = 12
description = "Persistent job queue with retry, exponential backoff, and Postgres LISTEN/NOTIFY."
+++

# Background Jobs

Blixt includes a persistent job queue backed by your database. Jobs survive
process restarts, retry automatically with exponential backoff, and on Postgres
get picked up near-instantly via `LISTEN`/`NOTIFY`.

## Enqueuing jobs

Use `Queue::enqueue()` to add a job from any handler:

```rust
use blixt::prelude::*;
use serde_json::json;

async fn register_user(
    State(ctx): State<AppContext>,
    Form(input): Form<RegisterInput>,
) -> Result<impl IntoResponse> {
    let user = create_user(&ctx.db, &input).await?;

    Queue::enqueue(&ctx.db, "send_welcome_email", json!({
        "user_id": user.id,
        "email": input.email,
    }))
    .run()
    .await?;

    Ok(Redirect::to("/"))
}
```

### Options

```rust
Queue::enqueue(&pool, "send_report", json!({"id": 42}))
    .queue("reports")           // queue name (default: "default")
    .max_attempts(10)           // retry limit (default: 5)
    .delay(Duration::from_secs(60))  // delay execution
    .run()
    .await?;
```

## Processing jobs

Create a `Worker`, register handlers, and run it:

```rust
use blixt::prelude::*;
use serde_json::Value;

let worker = Worker::new(job_pool)
    .queue("default")
    .concurrency(4)
    .register("send_welcome_email", |payload: Value| async move {
        let email = payload["email"].as_str().unwrap_or_default();
        // send the email...
        info!(email = %email, "welcome email sent");
        Ok(())
    })
    .register("send_report", |payload: Value| async move {
        // generate and send report...
        Ok(())
    });

// run blocks forever — spawn it in the background
tokio::spawn(worker.run());
```

The worker uses a separate database pool to avoid starving your app's connections:

```rust
let app_pool = db::create_pool(&config).await?;
let job_pool = db::create_pool(&config).await?;

tokio::spawn(Worker::new(job_pool).register(...).run());
App::new(config).db(app_pool).router(routes()).serve().await
```

## How it works

1. `Queue::enqueue()` inserts a row into the `_blixt_jobs` table
2. On Postgres, fires `NOTIFY _blixt_jobs` for immediate pickup
3. The worker polls for pending jobs (5 second interval, or instantly on Postgres via `LISTEN`)
4. Locks jobs with `FOR UPDATE SKIP LOCKED` (Postgres) to prevent double-processing
5. Executes the registered handler with the JSON payload

## Retries and failure

Failed jobs retry with exponential backoff: 30s, 1m, 2m, 4m, 8m, etc.

After `max_attempts` failures, the job is marked `dead` with the last error
message saved. Dead jobs stay in the table for inspection.

Job states: `pending` → `running` → `completed` or `dead`.

## The jobs table

The `_blixt_jobs` table is created automatically when the worker starts.
You don't need to add a migration.

```
id | queue | job_type | payload | status | attempts | max_attempts | last_error | scheduled_at
```

Unknown job types (no registered handler) are logged and marked `dead`.
