/// Parameterized SQL query that only accepts string literals.
///
/// Prevents SQL injection by rejecting runtime-constructed strings at
/// compile time. Use `.bind()` for all dynamic values.
///
/// # Example
///
/// ```rust,ignore
/// use blixt::prelude::*;
///
/// let todos = query!("SELECT id, title FROM todos WHERE id = ?")
///     .bind(id)
///     .fetch_one(&pool)
///     .await?;
/// ```
///
/// # Compile-time safety
///
/// ```compile_fail
/// // format!() is not a string literal -- rejected at compile time
/// let sql = format!("SELECT * FROM users WHERE name = '{}'", "test");
/// blixt::query!(sql);
/// ```
#[macro_export]
macro_rules! query {
    ($sql:literal) => {
        ::sqlx::query($sql)
    };
}

/// Parameterized SQL query with struct deserialization. Only accepts
/// string literals.
///
/// # Example
///
/// ```rust,ignore
/// let users = blixt::query_as!(User, "SELECT id, name FROM users")
///     .fetch_all(&pool)
///     .await?;
/// ```
///
/// ```compile_fail
/// let sql = String::from("SELECT * FROM users");
/// blixt::query_as!((), sql);
/// ```
#[macro_export]
macro_rules! query_as {
    ($T:ty, $sql:literal) => {
        ::sqlx::query_as::<_, $T>($sql)
    };
}

/// Parameterized scalar query. Only accepts string literals.
///
/// # Example
///
/// ```rust,ignore
/// let count: i64 = blixt::query_scalar!("SELECT COUNT(*) FROM users")
///     .fetch_one(&pool)
///     .await?;
/// ```
///
/// ```compile_fail
/// let sql = format!("SELECT COUNT(*) FROM {}", "users");
/// blixt::query_scalar!(sql);
/// ```
#[macro_export]
macro_rules! query_scalar {
    ($sql:literal) => {
        ::sqlx::query_scalar($sql)
    };
}

#[cfg(test)]
mod tests {
    // Compile-fail tests live in the doc comments above.
    //
    // The runtime tests verify that each macro compiles with a string
    // literal. We use inner `async fn` signatures that reference the
    // concrete `DbPool` so the database type parameter is resolved.

    use super::super::DbPool;

    #[test]
    fn query_macro_accepts_literal() {
        async fn _assert_compiles(pool: &DbPool) {
            let _ = crate::query!("SELECT 1").fetch_all(pool).await;
        }
    }

    #[test]
    fn query_as_macro_accepts_literal() {
        async fn _assert_compiles(pool: &DbPool) {
            let _ = crate::query_as!((i64,), "SELECT 1").fetch_all(pool).await;
        }
    }

    #[test]
    fn query_scalar_macro_accepts_literal() {
        async fn _assert_compiles(pool: &DbPool) {
            let _: Vec<i64> = crate::query_scalar!("SELECT 1")
                .fetch_all(pool)
                .await
                .unwrap();
        }
    }
}
