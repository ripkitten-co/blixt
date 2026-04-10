use askama::Template;
use axum::response::{Html, IntoResponse};
use blixt::prelude::*;

#[derive(Template)]
#[template(path = "pages/home.html")]
struct HelloPage {
    greeting: String,
}

async fn index() -> impl IntoResponse {
    Html(
        HelloPage {
            greeting: "Hello from Blixt!".to_string(),
        }
        .render()
        .unwrap_or_default(),
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;
    let config = Config::from_env()?;

    App::new(config)
        .router(Router::new().route("/", get(index)))
        .static_dir("static")
        .serve()
        .await
}
