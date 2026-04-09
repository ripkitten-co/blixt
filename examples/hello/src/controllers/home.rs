use askama::Template;
use axum::response::{Html, IntoResponse};

#[derive(Template)]
#[template(path = "pages/home.html")]
pub struct HomePage {
    pub name: String,
    pub port: u16,
    pub env: String,
}

pub async fn index() -> impl IntoResponse {
    let page = HomePage {
        name: "hello".to_string(),
        port: std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3000),
        env: std::env::var("BLIXT_ENV")
            .unwrap_or_else(|_| "development".into()),
    };
    Html(page.render().unwrap_or_default())
}
