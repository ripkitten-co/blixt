use sqlx::FromRow;

use crate::db::DbPool;
use crate::error::{Error, Result};

#[cfg(any(
    all(feature = "postgres", not(feature = "sqlite")),
    all(feature = "postgres", feature = "sqlite", docsrs),
))]
type Db = sqlx::Postgres;
#[cfg(all(feature = "sqlite", not(feature = "postgres"), not(docsrs)))]
type Db = sqlx::Sqlite;

type DbRow = <Db as sqlx::Database>::Row;

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum Dialect {
    Postgres,
    Sqlite,
}

#[cfg(any(
    all(feature = "postgres", not(feature = "sqlite")),
    all(feature = "postgres", feature = "sqlite", docsrs),
))]
const DIALECT: Dialect = Dialect::Postgres;
#[cfg(all(feature = "sqlite", not(feature = "postgres"), not(docsrs)))]
const DIALECT: Dialect = Dialect::Sqlite;

fn placeholder(n: usize) -> String {
    match DIALECT {
        Dialect::Postgres => format!("${n}"),
        Dialect::Sqlite => "?".to_string(),
    }
}

/// A value that can be bound to a SQL query.
#[derive(Debug, Clone)]
pub enum Value {
    /// A text value.
    String(String),
    /// A 64-bit integer.
    I64(i64),
    /// A 64-bit float.
    F64(f64),
    /// A boolean.
    Bool(bool),
    /// A null value.
    Null,
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_owned())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::I64(v)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::I64(v as i64)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::F64(v)
    }
}
impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::F64(v as f64)
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

struct Condition {
    column: &'static str,
    op: &'static str,
    value: Value,
}

struct InCondition {
    column: &'static str,
    values: Vec<Value>,
}

enum JoinKind {
    Inner,
    Left,
}

struct Join {
    kind: JoinKind,
    table: &'static str,
    on_left: &'static str,
    on_right: &'static str,
}

/// Sort direction for ORDER BY clauses.
#[derive(Debug, Clone, Copy)]
pub enum Order {
    /// Ascending order.
    Asc,
    /// Descending order.
    Desc,
}

fn bind_values<'q, T>(
    mut query: sqlx::query::QueryAs<'q, Db, T, <Db as sqlx::Database>::Arguments<'q>>,
    values: &'q [Value],
) -> sqlx::query::QueryAs<'q, Db, T, <Db as sqlx::Database>::Arguments<'q>>
where
    T: for<'r> FromRow<'r, DbRow>,
{
    for val in values {
        query = match val {
            Value::String(s) => query.bind(s.as_str()),
            Value::I64(v) => query.bind(*v),
            Value::F64(v) => query.bind(*v),
            Value::Bool(v) => query.bind(*v),
            Value::Null => query.bind(None::<String>),
        };
    }
    query
}

fn bind_values_exec<'q>(
    mut query: sqlx::query::Query<'q, Db, <Db as sqlx::Database>::Arguments<'q>>,
    values: &'q [Value],
) -> sqlx::query::Query<'q, Db, <Db as sqlx::Database>::Arguments<'q>> {
    for val in values {
        query = match val {
            Value::String(s) => query.bind(s.as_str()),
            Value::I64(v) => query.bind(*v),
            Value::F64(v) => query.bind(*v),
            Value::Bool(v) => query.bind(*v),
            Value::Null => query.bind(None::<String>),
        };
    }
    query
}

macro_rules! impl_where {
    ($ty:ty) => {
        impl $ty {
            /// Filter where column equals value.
            pub fn where_eq(mut self, column: &'static str, value: impl Into<Value>) -> Self {
                self.conditions.push(Condition {
                    column,
                    op: "=",
                    value: value.into(),
                });
                self
            }
            /// Filter where column is greater than value.
            pub fn where_gt(mut self, column: &'static str, value: impl Into<Value>) -> Self {
                self.conditions.push(Condition {
                    column,
                    op: ">",
                    value: value.into(),
                });
                self
            }
            /// Filter where column is less than value.
            pub fn where_lt(mut self, column: &'static str, value: impl Into<Value>) -> Self {
                self.conditions.push(Condition {
                    column,
                    op: "<",
                    value: value.into(),
                });
                self
            }
            /// Filter where column is greater than or equal to value.
            pub fn where_gte(mut self, column: &'static str, value: impl Into<Value>) -> Self {
                self.conditions.push(Condition {
                    column,
                    op: ">=",
                    value: value.into(),
                });
                self
            }
            /// Filter where column is less than or equal to value.
            pub fn where_lte(mut self, column: &'static str, value: impl Into<Value>) -> Self {
                self.conditions.push(Condition {
                    column,
                    op: "<=",
                    value: value.into(),
                });
                self
            }
            /// Filter where column does not equal value.
            pub fn where_ne(mut self, column: &'static str, value: impl Into<Value>) -> Self {
                self.conditions.push(Condition {
                    column,
                    op: "!=",
                    value: value.into(),
                });
                self
            }
            /// Filter where column value is in the given list.
            pub fn where_in(mut self, column: &'static str, values: Vec<Value>) -> Self {
                self.in_conditions.push(InCondition { column, values });
                self
            }
        }
    };
}

fn build_where_clause(
    conditions: &[Condition],
    in_conditions: &[InCondition],
    start_idx: usize,
) -> (String, usize) {
    if conditions.is_empty() && in_conditions.is_empty() {
        return (String::new(), start_idx);
    }
    let mut idx = start_idx;
    let mut parts: Vec<String> = Vec::new();

    for c in conditions {
        let p = placeholder(idx);
        idx += 1;
        parts.push(format!("{} {} {p}", c.column, c.op));
    }

    for ic in in_conditions {
        if ic.values.is_empty() {
            parts.push(format!("{} IN (NULL)", ic.column));
        } else {
            let placeholders: Vec<String> = ic
                .values
                .iter()
                .map(|_| {
                    let p = placeholder(idx);
                    idx += 1;
                    p
                })
                .collect();
            parts.push(format!("{} IN ({})", ic.column, placeholders.join(", ")));
        }
    }

    (format!(" WHERE {}", parts.join(" AND ")), idx)
}

fn condition_values(conditions: &[Condition]) -> Vec<Value> {
    conditions.iter().map(|c| c.value.clone()).collect()
}

fn in_condition_values(in_conditions: &[InCondition]) -> Vec<Value> {
    in_conditions
        .iter()
        .flat_map(|ic| ic.values.clone())
        .collect()
}

/// Query builder for SELECT statements.
pub struct Select {
    table: &'static str,
    columns: Vec<&'static str>,
    conditions: Vec<Condition>,
    in_conditions: Vec<InCondition>,
    joins: Vec<Join>,
    order: Option<(&'static str, Order)>,
    limit_val: Option<i64>,
    offset_val: Option<i64>,
}

impl Select {
    /// Start a SELECT from the given table.
    pub fn from(table: &'static str) -> Self {
        Self {
            table,
            columns: Vec::new(),
            conditions: Vec::new(),
            in_conditions: Vec::new(),
            joins: Vec::new(),
            order: None,
            limit_val: None,
            offset_val: None,
        }
    }

    /// Set which columns to select.
    pub fn columns(mut self, cols: &[&'static str]) -> Self {
        self.columns = cols.to_vec();
        self
    }

    /// Sort by a column.
    pub fn order_by(mut self, column: &'static str, order: Order) -> Self {
        self.order = Some((column, order));
        self
    }

    /// Limit the number of rows returned.
    pub fn limit(mut self, n: i64) -> Self {
        self.limit_val = Some(n);
        self
    }

    /// Skip the first N rows.
    pub fn offset(mut self, n: i64) -> Self {
        self.offset_val = Some(n);
        self
    }

    /// Add an INNER JOIN clause.
    pub fn join(
        mut self,
        table: &'static str,
        on_left: &'static str,
        on_right: &'static str,
    ) -> Self {
        self.joins.push(Join {
            kind: JoinKind::Inner,
            table,
            on_left,
            on_right,
        });
        self
    }

    /// Add a LEFT JOIN clause.
    pub fn left_join(
        mut self,
        table: &'static str,
        on_left: &'static str,
        on_right: &'static str,
    ) -> Self {
        self.joins.push(Join {
            kind: JoinKind::Left,
            table,
            on_left,
            on_right,
        });
        self
    }

    fn to_sql(&self) -> String {
        let cols = if self.columns.is_empty() {
            "*".to_string()
        } else {
            self.columns.join(", ")
        };
        let mut sql = format!("SELECT {cols} FROM {}", self.table);

        for j in &self.joins {
            let kind = match j.kind {
                JoinKind::Inner => "JOIN",
                JoinKind::Left => "LEFT JOIN",
            };
            sql.push_str(&format!(
                " {kind} {} ON {} = {}",
                j.table, j.on_left, j.on_right
            ));
        }

        let (where_clause, mut idx) = build_where_clause(&self.conditions, &self.in_conditions, 1);
        sql.push_str(&where_clause);

        if let Some((col, order)) = &self.order {
            let dir = match order {
                Order::Asc => "ASC",
                Order::Desc => "DESC",
            };
            sql.push_str(&format!(" ORDER BY {col} {dir}"));
        }
        if self.limit_val.is_some() {
            sql.push_str(&format!(" LIMIT {}", placeholder(idx)));
            idx += 1;
        }
        if self.offset_val.is_some() {
            sql.push_str(&format!(" OFFSET {}", placeholder(idx)));
        }
        sql
    }

    fn all_values(&self) -> Vec<Value> {
        let mut vals = condition_values(&self.conditions);
        vals.extend(in_condition_values(&self.in_conditions));
        if let Some(limit) = self.limit_val {
            vals.push(Value::I64(limit));
        }
        if let Some(offset) = self.offset_val {
            vals.push(Value::I64(offset));
        }
        vals
    }

    /// Fetch all matching rows.
    pub async fn fetch_all<T>(self, pool: &DbPool) -> Result<Vec<T>>
    where
        T: for<'r> FromRow<'r, DbRow> + Send + Unpin,
    {
        let sql = self.to_sql();
        let values = self.all_values();
        let query = bind_values(sqlx::query_as::<Db, T>(&sql), &values);
        Ok(query.fetch_all(pool).await?)
    }

    /// Fetch exactly one row. Returns `Error::NotFound` if no match.
    pub async fn fetch_one<T>(self, pool: &DbPool) -> Result<T>
    where
        T: for<'r> FromRow<'r, DbRow> + Send + Unpin,
    {
        self.fetch_optional::<T>(pool).await?.ok_or(Error::NotFound)
    }

    /// Fetch zero or one row.
    pub async fn fetch_optional<T>(self, pool: &DbPool) -> Result<Option<T>>
    where
        T: for<'r> FromRow<'r, DbRow> + Send + Unpin,
    {
        let sql = self.to_sql();
        let values = self.all_values();
        let query = bind_values(sqlx::query_as::<Db, T>(&sql), &values);
        Ok(query.fetch_optional(pool).await?)
    }
}

impl_where!(Select);

/// Query builder for INSERT statements.
pub struct Insert {
    table: &'static str,
    fields: Vec<(&'static str, Value)>,
}

/// An INSERT with a RETURNING clause.
pub struct InsertReturning<T> {
    insert: Insert,
    columns: Vec<&'static str>,
    _marker: std::marker::PhantomData<T>,
}

impl Insert {
    /// Start an INSERT into the given table.
    pub fn into(table: &'static str) -> Self {
        Self {
            table,
            fields: Vec::new(),
        }
    }

    /// Set a column value.
    pub fn set(mut self, column: &'static str, value: impl Into<Value>) -> Self {
        self.fields.push((column, value.into()));
        self
    }

    /// Add a RETURNING clause to get the inserted row back.
    pub fn returning<T>(self, columns: &[&'static str]) -> InsertReturning<T> {
        InsertReturning {
            insert: self,
            columns: columns.to_vec(),
            _marker: std::marker::PhantomData,
        }
    }

    fn to_sql(&self) -> String {
        let cols: Vec<&str> = self.fields.iter().map(|(c, _)| *c).collect();
        let placeholders: Vec<String> = (1..=self.fields.len()).map(placeholder).collect();
        format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.table,
            cols.join(", "),
            placeholders.join(", ")
        )
    }

    fn values(&self) -> Vec<Value> {
        self.fields.iter().map(|(_, v)| v.clone()).collect()
    }

    /// Execute the insert without returning a row.
    pub async fn execute_no_return(self, pool: &DbPool) -> Result<()> {
        let sql = self.to_sql();
        let values = self.values();
        let query = bind_values_exec(sqlx::query::<Db>(&sql), &values);
        query.execute(pool).await?;
        Ok(())
    }
}

impl<T> InsertReturning<T>
where
    T: for<'r> FromRow<'r, DbRow> + Send + Unpin,
{
    /// Execute the insert and return the created row.
    pub async fn execute(self, pool: &DbPool) -> Result<T> {
        let sql = format!(
            "{} RETURNING {}",
            self.insert.to_sql(),
            self.columns.join(", ")
        );
        let values = self.insert.values();
        let query = bind_values(sqlx::query_as::<Db, T>(&sql), &values);
        Ok(query.fetch_one(pool).await?)
    }
}

/// Query builder for UPDATE statements.
pub struct Update {
    table: &'static str,
    fields: Vec<(&'static str, Value)>,
    conditions: Vec<Condition>,
    in_conditions: Vec<InCondition>,
    timestamp_cols: Vec<&'static str>,
}

/// An UPDATE with a RETURNING clause.
pub struct UpdateReturning<T> {
    update: Update,
    columns: Vec<&'static str>,
    _marker: std::marker::PhantomData<T>,
}

impl Update {
    /// Start an UPDATE on the given table.
    pub fn table(table: &'static str) -> Self {
        Self {
            table,
            fields: Vec::new(),
            conditions: Vec::new(),
            in_conditions: Vec::new(),
            timestamp_cols: Vec::new(),
        }
    }

    /// Set a column to a new value.
    pub fn set(mut self, column: &'static str, value: impl Into<Value>) -> Self {
        self.fields.push((column, value.into()));
        self
    }

    /// Set a column to CURRENT_TIMESTAMP.
    pub fn set_timestamp(mut self, column: &'static str) -> Self {
        self.timestamp_cols.push(column);
        self
    }

    /// Add a RETURNING clause.
    pub fn returning<T>(self, columns: &[&'static str]) -> UpdateReturning<T> {
        UpdateReturning {
            update: self,
            columns: columns.to_vec(),
            _marker: std::marker::PhantomData,
        }
    }

    fn to_sql(&self) -> String {
        let mut sets: Vec<String> = self
            .fields
            .iter()
            .enumerate()
            .map(|(i, (col, _))| format!("{col} = {}", placeholder(i + 1)))
            .collect();
        for ts in &self.timestamp_cols {
            sets.push(format!("{ts} = CURRENT_TIMESTAMP"));
        }
        let mut sql = format!("UPDATE {} SET {}", self.table, sets.join(", "));
        let offset = self.fields.len();
        let (where_clause, _) =
            build_where_clause(&self.conditions, &self.in_conditions, offset + 1);
        sql.push_str(&where_clause);
        sql
    }

    fn all_values(&self) -> Vec<Value> {
        let mut vals: Vec<Value> = self.fields.iter().map(|(_, v)| v.clone()).collect();
        vals.extend(condition_values(&self.conditions));
        vals.extend(in_condition_values(&self.in_conditions));
        vals
    }

    /// Execute without returning a row.
    pub async fn execute_no_return(self, pool: &DbPool) -> Result<()> {
        let sql = self.to_sql();
        let values = self.all_values();
        let query = bind_values_exec(sqlx::query::<Db>(&sql), &values);
        query.execute(pool).await?;
        Ok(())
    }
}

impl_where!(Update);

impl<T> UpdateReturning<T>
where
    T: for<'r> FromRow<'r, DbRow> + Send + Unpin,
{
    /// Execute and return the updated row.
    pub async fn execute(self, pool: &DbPool) -> Result<T> {
        let sql = format!(
            "{} RETURNING {}",
            self.update.to_sql(),
            self.columns.join(", ")
        );
        let values = self.update.all_values();
        let query = bind_values(sqlx::query_as::<Db, T>(&sql), &values);
        Ok(query.fetch_one(pool).await?)
    }
}

/// Query builder for DELETE statements.
pub struct Delete {
    table: &'static str,
    conditions: Vec<Condition>,
    in_conditions: Vec<InCondition>,
}

impl Delete {
    /// Start a DELETE from the given table.
    pub fn from(table: &'static str) -> Self {
        Self {
            table,
            conditions: Vec::new(),
            in_conditions: Vec::new(),
        }
    }

    /// Execute the delete.
    pub async fn execute(self, pool: &DbPool) -> Result<()> {
        if self.conditions.is_empty() && self.in_conditions.is_empty() {
            tracing::warn!(table = self.table, "DELETE without WHERE conditions");
        }
        let mut sql = format!("DELETE FROM {}", self.table);
        let (where_clause, _) = build_where_clause(&self.conditions, &self.in_conditions, 1);
        sql.push_str(&where_clause);
        let mut values = condition_values(&self.conditions);
        values.extend(in_condition_values(&self.in_conditions));
        let query = bind_values_exec(sqlx::query::<Db>(&sql), &values);
        query.execute(pool).await?;
        Ok(())
    }
}

impl_where!(Delete);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_from_str() {
        let v: Value = "hello".into();
        assert!(matches!(v, Value::String(s) if s == "hello"));
    }

    #[test]
    fn value_from_i64() {
        let v: Value = 42i64.into();
        assert!(matches!(v, Value::I64(42)));
    }

    #[test]
    fn value_from_i32() {
        let v: Value = 42i32.into();
        assert!(matches!(v, Value::I64(42)));
    }

    #[test]
    fn value_from_bool() {
        let v: Value = true.into();
        assert!(matches!(v, Value::Bool(true)));
    }

    #[test]
    fn value_from_f64() {
        let v: Value = 3.14f64.into();
        assert!(matches!(v, Value::F64(f) if (f - 3.14).abs() < f64::EPSILON));
    }

    #[test]
    fn select_generates_sql() {
        let s = Select::from("posts")
            .columns(&["id", "title"])
            .where_eq("published", true)
            .order_by("created_at", Order::Desc)
            .limit(10);
        let sql = s.to_sql();
        assert!(sql.starts_with("SELECT id, title FROM posts"));
        assert!(sql.contains("WHERE published"));
        assert!(sql.contains("ORDER BY created_at DESC"));
        assert!(sql.contains("LIMIT"));
    }

    #[test]
    fn insert_generates_sql() {
        let i = Insert::into("posts")
            .set("title", "Hello")
            .set("body", "World");
        let sql = i.to_sql();
        assert!(sql.starts_with("INSERT INTO posts (title, body) VALUES ("));
    }

    #[test]
    fn update_generates_sql() {
        let u = Update::table("posts")
            .set("title", "New")
            .set_timestamp("updated_at")
            .where_eq("id", 1i64);
        let sql = u.to_sql();
        assert!(sql.starts_with("UPDATE posts SET"));
        assert!(sql.contains("updated_at = CURRENT_TIMESTAMP"));
        assert!(sql.contains("WHERE id"));
    }

    #[test]
    fn where_in_generates_sql() {
        let s =
            Select::from("posts").where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)]);
        let sql = s.to_sql();
        assert!(sql.contains("WHERE id IN ("));
    }

    #[test]
    fn where_in_empty_generates_null() {
        let s = Select::from("posts").where_in("id", vec![]);
        let sql = s.to_sql();
        assert!(sql.contains("WHERE id IN (NULL)"));
    }

    #[test]
    fn where_in_combined_with_where_eq() {
        let s = Select::from("posts")
            .where_eq("published", true)
            .where_in("id", vec![Value::I64(1), Value::I64(2)]);
        let sql = s.to_sql();
        assert!(sql.contains("WHERE published"));
        assert!(sql.contains("AND id IN ("));
    }

    #[test]
    fn join_generates_inner_join_sql() {
        let s = Select::from("posts")
            .join("users", "users.id", "posts.author_id")
            .columns(&["posts.*", "users.name"]);
        let sql = s.to_sql();
        assert!(sql.contains("JOIN users ON users.id = posts.author_id"));
        assert!(!sql.contains("LEFT"));
    }

    #[test]
    fn left_join_generates_sql() {
        let s = Select::from("posts").left_join("categories", "categories.id", "posts.category_id");
        let sql = s.to_sql();
        assert!(sql.contains("LEFT JOIN categories ON categories.id = posts.category_id"));
    }

    #[test]
    fn multiple_joins() {
        let s = Select::from("posts")
            .join("users", "users.id", "posts.author_id")
            .left_join("categories", "categories.id", "posts.category_id");
        let sql = s.to_sql();
        assert!(sql.contains("JOIN users"));
        assert!(sql.contains("LEFT JOIN categories"));
    }

    #[test]
    fn join_with_where() {
        let s = Select::from("posts")
            .join("users", "users.id", "posts.author_id")
            .where_eq("users.role", "admin");
        let sql = s.to_sql();
        assert!(sql.contains("JOIN users"));
        assert!(sql.contains("WHERE users.role"));
    }

    #[cfg(feature = "sqlite")]
    mod db_tests {
        use super::super::*;
        use crate::config::{Config, Environment};
        use crate::db::create_pool;

        async fn test_pool() -> DbPool {
            let config = Config {
                host: "127.0.0.1".to_string(),
                port: 3000,
                blixt_env: Environment::Test,
                database_url: Some(secrecy::SecretString::from("sqlite::memory:".to_string())),
                jwt_secret: None,
            };
            let pool = create_pool(&config).await.expect("pool");
            sqlx::query("CREATE TABLE test_items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, score INTEGER NOT NULL)")
                .execute(&pool).await.expect("create table");
            sqlx::query("INSERT INTO test_items (id, name, score) VALUES (1, 'alpha', 10), (2, 'beta', 20), (3, 'gamma', 30)")
                .execute(&pool).await.expect("seed");
            pool
        }

        #[derive(Debug, sqlx::FromRow, PartialEq)]
        struct TestItem {
            id: i64,
            name: String,
            score: i64,
        }

        #[tokio::test]
        async fn select_fetch_all() {
            let pool = test_pool().await;
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .order_by("id", Order::Asc)
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 3);
            assert_eq!(items[0].name, "alpha");
        }

        #[tokio::test]
        async fn select_fetch_one_with_where() {
            let pool = test_pool().await;
            let item = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .where_eq("id", 2i64)
                .fetch_one::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(item.name, "beta");
        }

        #[tokio::test]
        async fn select_fetch_one_not_found() {
            let pool = test_pool().await;
            let result = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .where_eq("id", 999i64)
                .fetch_one::<TestItem>(&pool)
                .await;
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn select_fetch_optional_none() {
            let pool = test_pool().await;
            let result = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .where_eq("id", 999i64)
                .fetch_optional::<TestItem>(&pool)
                .await
                .unwrap();
            assert!(result.is_none());
        }

        #[tokio::test]
        async fn select_with_gt_and_order() {
            let pool = test_pool().await;
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .where_gt("score", 10i64)
                .order_by("score", Order::Desc)
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].name, "gamma");
        }

        #[tokio::test]
        async fn select_with_limit() {
            let pool = test_pool().await;
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .order_by("id", Order::Asc)
                .limit(2)
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 2);
        }

        #[tokio::test]
        async fn select_with_limit_and_offset() {
            let pool = test_pool().await;
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .order_by("id", Order::Asc)
                .limit(2)
                .offset(1)
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].name, "beta");
        }

        #[tokio::test]
        async fn insert_and_return() {
            let pool = test_pool().await;
            let item = Insert::into("test_items")
                .set("name", "delta")
                .set("score", 40i64)
                .returning::<TestItem>(&["id", "name", "score"])
                .execute(&pool)
                .await
                .unwrap();
            assert_eq!(item.name, "delta");
            assert_eq!(item.score, 40);
        }

        #[tokio::test]
        async fn insert_no_return() {
            let pool = test_pool().await;
            Insert::into("test_items")
                .set("name", "epsilon")
                .set("score", 50i64)
                .execute_no_return(&pool)
                .await
                .unwrap();
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 4);
        }

        #[tokio::test]
        async fn update_with_returning() {
            let pool = test_pool().await;
            let item = Update::table("test_items")
                .set("name", "ALPHA")
                .set("score", 100i64)
                .where_eq("id", 1i64)
                .returning::<TestItem>(&["id", "name", "score"])
                .execute(&pool)
                .await
                .unwrap();
            assert_eq!(item.name, "ALPHA");
            assert_eq!(item.score, 100);
        }

        #[tokio::test]
        async fn update_no_return() {
            let pool = test_pool().await;
            Update::table("test_items")
                .set("score", 0i64)
                .where_gt("score", 10i64)
                .execute_no_return(&pool)
                .await
                .unwrap();
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .where_eq("score", 0i64)
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 2);
        }

        #[tokio::test]
        async fn delete_single_row() {
            let pool = test_pool().await;
            Delete::from("test_items")
                .where_eq("id", 1i64)
                .execute(&pool)
                .await
                .unwrap();
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 2);
        }

        #[tokio::test]
        async fn delete_with_condition() {
            let pool = test_pool().await;
            Delete::from("test_items")
                .where_lt("score", 25i64)
                .execute(&pool)
                .await
                .unwrap();
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].name, "gamma");
        }

        #[tokio::test]
        async fn where_in_fetches_matching_rows() {
            let pool = test_pool().await;
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .where_in("id", vec![Value::I64(1), Value::I64(3)])
                .order_by("id", Order::Asc)
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].name, "alpha");
            assert_eq!(items[1].name, "gamma");
        }

        #[tokio::test]
        async fn where_in_with_no_matches() {
            let pool = test_pool().await;
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .where_in("id", vec![Value::I64(99), Value::I64(100)])
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 0);
        }

        #[tokio::test]
        async fn where_in_empty_returns_nothing() {
            let pool = test_pool().await;
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .where_in("id", vec![])
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 0);
        }

        #[tokio::test]
        async fn delete_with_where_in() {
            let pool = test_pool().await;
            Delete::from("test_items")
                .where_in("id", vec![Value::I64(1), Value::I64(2)])
                .execute(&pool)
                .await
                .unwrap();
            let items = Select::from("test_items")
                .columns(&["id", "name", "score"])
                .fetch_all::<TestItem>(&pool)
                .await
                .unwrap();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].name, "gamma");
        }

        #[tokio::test]
        async fn join_fetches_combined_rows() {
            let pool = test_pool().await;
            sqlx::query(
                "CREATE TABLE test_categories (id INTEGER PRIMARY KEY, label TEXT NOT NULL)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO test_categories (id, label) VALUES (1, 'A'), (2, 'B')")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE test_tagged (id INTEGER PRIMARY KEY, name TEXT NOT NULL, cat_id INTEGER NOT NULL)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO test_tagged (id, name, cat_id) VALUES (1, 'x', 1), (2, 'y', 2)",
            )
            .execute(&pool)
            .await
            .unwrap();

            #[derive(Debug, sqlx::FromRow)]
            struct TaggedWithCat {
                name: String,
                label: String,
            }

            let rows = Select::from("test_tagged")
                .join(
                    "test_categories",
                    "test_categories.id",
                    "test_tagged.cat_id",
                )
                .columns(&["test_tagged.name", "test_categories.label"])
                .order_by("test_tagged.id", Order::Asc)
                .fetch_all::<TaggedWithCat>(&pool)
                .await
                .unwrap();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].name, "x");
            assert_eq!(rows[0].label, "A");
        }

        #[tokio::test]
        async fn left_join_includes_nulls() {
            let pool = test_pool().await;
            sqlx::query("CREATE TABLE test_parents (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO test_parents (id, name) VALUES (1, 'p1'), (2, 'p2')")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE test_children (id INTEGER PRIMARY KEY, parent_id INTEGER, val TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO test_children (id, parent_id, val) VALUES (1, 1, 'c1')")
                .execute(&pool)
                .await
                .unwrap();

            #[derive(Debug, sqlx::FromRow)]
            struct ParentChild {
                name: String,
                val: Option<String>,
            }

            let rows = Select::from("test_parents")
                .left_join(
                    "test_children",
                    "test_children.parent_id",
                    "test_parents.id",
                )
                .columns(&["test_parents.name", "test_children.val"])
                .order_by("test_parents.id", Order::Asc)
                .fetch_all::<ParentChild>(&pool)
                .await
                .unwrap();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].val, Some("c1".to_string()));
            assert!(rows[1].val.is_none());
        }
    }
}
