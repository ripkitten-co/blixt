use askama::Template;
use axum::response::{Html, IntoResponse, Response};
use blixt::datastar::{SseFragment, DatastarSignals};
use blixt::prelude::*;

#[derive(Debug, Clone, FromRow, Serialize)]
struct Todo {
    id: i64,
    title: String,
    completed: bool,
}

// -- templates --

#[derive(Template)]
#[template(path = "index.html")]
struct IndexPage {
    todos: Vec<Todo>,
}

#[derive(Template)]
#[template(path = "fragments/todo_list.html")]
struct TodoListFragment {
    todos: Vec<Todo>,
}

// -- handlers --

async fn index(State(ctx): State<AppContext>) -> impl IntoResponse {
    let todos = fetch_todos(&ctx.db).await;
    Html(IndexPage { todos }.render().unwrap_or_default())
}

async fn create(
    State(ctx): State<AppContext>,
    signals: DatastarSignals,
) -> Response {
    let title: String = match signals.get("title") {
        Ok(t) => t,
        Err(_) => return bad_request(),
    };
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return bad_request();
    }

    let _ = sqlx::query("INSERT INTO todos (title) VALUES (?)")
        .bind(trimmed)
        .execute(&ctx.db)
        .await;

    todo_list_fragment(&ctx.db).await
}

async fn toggle(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
) -> Response {
    let _ = sqlx::query("UPDATE todos SET completed = NOT completed WHERE id = ?")
        .bind(id)
        .execute(&ctx.db)
        .await;

    todo_list_fragment(&ctx.db).await
}

async fn destroy(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
) -> Response {
    let _ = sqlx::query("DELETE FROM todos WHERE id = ?")
        .bind(id)
        .execute(&ctx.db)
        .await;

    todo_list_fragment(&ctx.db).await
}

// -- helpers --

async fn fetch_todos(db: &DbPool) -> Vec<Todo> {
    sqlx::query_as::<_, Todo>("SELECT id, title, completed FROM todos ORDER BY id")
        .fetch_all(db)
        .await
        .unwrap_or_default()
}

async fn todo_list_fragment(db: &DbPool) -> Response {
    let todos = fetch_todos(db).await;
    SseFragment::new(TodoListFragment { todos })
        .expect("render fragment")
        .into_response()
}

fn bad_request() -> Response {
    axum::http::StatusCode::BAD_REQUEST.into_response()
}

// -- main --

fn routes(ctx: AppContext) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/todos", post(create))
        .route("/todos/{id}/toggle", put(toggle))
        .route("/todos/{id}", delete(destroy))
        .with_state(ctx)
}

async fn run_migrations(db: &DbPool) {
    let sql = include_str!("../migrations/001_create_todos.sql");
    sqlx::query(sql)
        .execute(db)
        .await
        .expect("failed to run migrations");
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let config = Config::from_env()?;
    let db = blixt::db::create_pool(&config).await?;
    run_migrations(&db).await;

    let ctx = AppContext::new(db, config);
    let app_config = Config::from_env()?;

    App::new(app_config)
        .router(routes(ctx))
        .static_dir("static")
        .serve()
        .await
}
