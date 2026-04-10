use blixt::prelude::*;
use serde_json::json;

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

async fn fetch_todos(pool: &DbPool) -> Result<Vec<Todo>> {
    Ok(query_as!(
        Todo,
        "SELECT id, title, completed FROM todos ORDER BY id DESC"
    )
    .fetch_all(pool)
    .await?)
}

async fn index(State(ctx): State<AppContext>) -> Result<impl IntoResponse> {
    let todos = fetch_todos(&ctx.db).await?;
    let html = HomePage { todos }
        .render()
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(Html(html))
}

async fn create(
    State(ctx): State<AppContext>,
    signals: DatastarSignals,
) -> Result<impl IntoResponse> {
    let title: String = signals.get("title")?;
    let title = title.trim().to_owned();
    if title.is_empty() {
        return SseResponse::new().signals(&json!({"title": ""}));
    }
    query!("INSERT INTO todos (title) VALUES (?)")
        .bind(&title)
        .execute(&ctx.db)
        .await?;
    let todos = fetch_todos(&ctx.db).await?;
    SseResponse::new()
        .patch(TodoListFragment { todos })?
        .signals(&json!({"title": ""}))
}

async fn toggle(State(ctx): State<AppContext>, Path(id): Path<i64>) -> Result<impl IntoResponse> {
    query!("UPDATE todos SET completed = NOT completed WHERE id = ?")
        .bind(id)
        .execute(&ctx.db)
        .await?;
    let todos = fetch_todos(&ctx.db).await?;
    SseFragment::new(TodoListFragment { todos })
}

async fn remove(State(ctx): State<AppContext>, Path(id): Path<i64>) -> Result<impl IntoResponse> {
    query!("DELETE FROM todos WHERE id = ?")
        .bind(id)
        .execute(&ctx.db)
        .await?;
    let todos = fetch_todos(&ctx.db).await?;
    SseFragment::new(TodoListFragment { todos })
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
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| Error::Internal(format!("migration failed: {e}")))?;
    let ctx = AppContext::new(pool, config.clone());
    App::new(config)
        .router(routes().with_state(ctx))
        .static_dir("static")
        .serve()
        .await
}
