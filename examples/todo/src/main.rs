use axum::http::header;
use axum::response::{Html, Response};
use blixt::datastar::DatastarSignals;
use blixt::prelude::*;

#[derive(Debug, Clone, sqlx::FromRow)]
struct Todo {
    id: i64,
    title: String,
    completed: bool,
}

#[derive(Template)]
#[template(path = "pages/home.html")]
struct HomePage {
    todos: Vec<Todo>,
}

#[derive(Template)]
#[template(path = "fragments/todo_list.html")]
struct TodoListFragment {
    todos: Vec<Todo>,
}

fn sse_patch(html: &str) -> Response {
    let oneline: String = html.trim().lines().map(str::trim).collect();
    let body = format!("event: datastar-patch-elements\ndata: elements {oneline}\n\n");
    (
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response()
}

fn sse_signals(json: &str) -> Response {
    let body = format!("event: datastar-patch-signals\ndata: signals {json}\n\n");
    (
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response()
}

async fn fetch_todos(pool: &DbPool) -> Vec<Todo> {
    sqlx::query_as::<_, Todo>("SELECT id, title, completed FROM todos ORDER BY id DESC")
        .fetch_all(pool)
        .await
        .unwrap_or_default()
}

async fn index(State(ctx): State<AppContext>) -> impl IntoResponse {
    let todos = fetch_todos(&ctx.db).await;
    Html(HomePage { todos }.render().unwrap_or_default())
}

async fn create(
    State(ctx): State<AppContext>,
    signals: DatastarSignals,
) -> Response {
    let title: String = signals.get("title").unwrap_or_default();
    let title = title.trim();
    if title.is_empty() {
        return sse_signals(r#"{"title":""}"#);
    }

    sqlx::query("INSERT INTO todos (title) VALUES (?)")
        .bind(title)
        .execute(&ctx.db)
        .await
        .ok();

    let todos = fetch_todos(&ctx.db).await;
    let list_html = TodoListFragment { todos }.render().unwrap_or_default();
    let list_patch = {
        let oneline: String = list_html.trim().lines().map(str::trim).collect();
        format!("event: datastar-patch-elements\ndata: elements {oneline}\n\n")
    };
    let signals_patch = "event: datastar-patch-signals\ndata: signals {\"title\":\"\"}\n\n";

    (
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        format!("{list_patch}{signals_patch}"),
    )
        .into_response()
}

async fn toggle(State(ctx): State<AppContext>, Path(id): Path<i64>) -> Response {
    sqlx::query("UPDATE todos SET completed = NOT completed WHERE id = ?")
        .bind(id)
        .execute(&ctx.db)
        .await
        .ok();

    let todos = fetch_todos(&ctx.db).await;
    sse_patch(&TodoListFragment { todos }.render().unwrap_or_default())
}

async fn remove(State(ctx): State<AppContext>, Path(id): Path<i64>) -> Response {
    sqlx::query("DELETE FROM todos WHERE id = ?")
        .bind(id)
        .execute(&ctx.db)
        .await
        .ok();

    let todos = fetch_todos(&ctx.db).await;
    sse_patch(&TodoListFragment { todos }.render().unwrap_or_default())
}

fn routes() -> Router<AppContext> {
    Router::new()
        .route("/", get(index))
        .route("/todos", post(create))
        .route("/todos/{id}/toggle", put(toggle))
        .route("/todos/{id}", delete(remove))
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let config = Config::from_env()?;
    let pool = blixt::db::create_pool(&config).await?;

    sqlx::migrate!("./migrations").run(&pool).await.map_err(|e| {
        blixt::error::Error::Internal(format!("migration failed: {e}"))
    })?;

    let ctx = AppContext::new(pool, config);

    let app = App::new(Config::from_env()?)
        .router(routes().with_state(ctx))
        .static_dir("static");

    app.serve().await
}
