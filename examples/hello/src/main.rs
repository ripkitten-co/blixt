use blixt::prelude::*;

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
