use askama::Template;
use axum::response::{Html, IntoResponse};
use blixt::prelude::*;

#[derive(Template)]
#[template(path = "hello.html")]
struct Hello {
    name: String,
}

async fn index() -> impl IntoResponse {
    let page = Hello {
        name: "world".into(),
    };
    Html(page.render().unwrap_or_default())
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
