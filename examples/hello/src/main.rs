use blixt::prelude::*;

#[derive(Template)]
#[template(path = "pages/home.html")]
struct HelloPage {
    greeting: String,
}

async fn index() -> Result<impl IntoResponse> {
    render!(HelloPage {
        greeting: "Hello from Blixt!".to_string(),
    })
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
