use blixt::prelude::*;
use blixt::validate::Validator;
use serde_json::json;

const PER_PAGE: u32 = 5;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct Todo {
    id: i64,
    title: String,
    completed: bool,
}

#[derive(Template)]
#[template(path = "pages/home.html")]
struct HomePage {
    page: Paginated<Todo>,
}

#[derive(Template)]
#[template(path = "fragments/todo_list.html")]
struct TodoListFragment {
    page: Paginated<Todo>,
}

async fn fetch_page(pool: &DbPool, page_num: u32) -> Result<Paginated<Todo>> {
    Paginated::<Todo>::query(
        "SELECT id, title, completed FROM todos ORDER BY id DESC",
        pool,
        &PaginationParams::new(page_num, PER_PAGE),
    )
    .await
}

async fn index(
    State(ctx): State<AppContext>,
    pagination: PaginationParams,
) -> Result<impl IntoResponse> {
    let page = Paginated::<Todo>::query(
        "SELECT id, title, completed FROM todos ORDER BY id DESC",
        &ctx.db,
        &PaginationParams::new(pagination.page(), PER_PAGE),
    )
    .await?;
    let html = HomePage { page }
        .render()
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(Html(html))
}

async fn page_handler(
    State(ctx): State<AppContext>,
    pagination: PaginationParams,
) -> Result<impl IntoResponse> {
    let page = Paginated::<Todo>::query(
        "SELECT id, title, completed FROM todos ORDER BY id DESC",
        &ctx.db,
        &PaginationParams::new(pagination.page(), PER_PAGE),
    )
    .await?;
    SseFragment::new(TodoListFragment { page })
}

async fn create(
    State(ctx): State<AppContext>,
    signals: DatastarSignals,
) -> Result<impl IntoResponse> {
    let title: String = signals.get("title")?;
    let mut v = Validator::new();
    v.str_field(&title, "title").not_empty().max_length(255);
    v.check()?;
    let title = title.trim().to_owned();
    query!("INSERT INTO todos (title) VALUES (?)")
        .bind(&title)
        .execute(&ctx.db)
        .await?;
    let page = fetch_page(&ctx.db, 1).await?;
    SseResponse::new()
        .patch(TodoListFragment { page })?
        .signals(&json!({"title": ""}))
}

async fn toggle(State(ctx): State<AppContext>, Path(id): Path<i64>) -> Result<impl IntoResponse> {
    query!("UPDATE todos SET completed = NOT completed WHERE id = ?")
        .bind(id)
        .execute(&ctx.db)
        .await?;
    let page = fetch_page(&ctx.db, 1).await?;
    SseFragment::new(TodoListFragment { page })
}

async fn remove(State(ctx): State<AppContext>, Path(id): Path<i64>) -> Result<impl IntoResponse> {
    query!("DELETE FROM todos WHERE id = ?")
        .bind(id)
        .execute(&ctx.db)
        .await?;
    let page = fetch_page(&ctx.db, 1).await?;
    SseFragment::new(TodoListFragment { page })
}

fn routes() -> Router<AppContext> {
    Router::new()
        .route("/", get(index))
        .route("/todos", post(create))
        .route("/todos/page", get(page_handler))
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
