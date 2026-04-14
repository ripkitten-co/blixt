use std::collections::HashMap;

use sqlx::FromRow;

use super::DbPool;
use super::builder::{Select, Value};
use crate::error::Result;

#[cfg(any(
    all(feature = "postgres", not(feature = "sqlite")),
    all(feature = "postgres", feature = "sqlite", docsrs),
))]
type DbRow = <sqlx::Postgres as sqlx::Database>::Row;
#[cfg(all(feature = "sqlite", not(feature = "postgres"), not(docsrs)))]
type DbRow = <sqlx::Sqlite as sqlx::Database>::Row;

/// Implemented by models that have an integer primary key.
pub trait HasId {
    /// Returns the primary key value.
    fn id(&self) -> i64;
}

/// Declares a many-to-one relationship: Self has a foreign key to Parent.
pub trait BelongsTo<Parent> {
    /// Column name on Self's table holding the foreign key.
    const FOREIGN_KEY: &'static str;
    /// Table name of the parent model.
    const PARENT_TABLE: &'static str;

    /// Returns the foreign key value for this record.
    fn fk_value(&self) -> i64;
}

/// Declares a one-to-one relationship where the foreign key is on the child.
pub trait HasOne<Child> {
    /// Column name on the child's table holding the foreign key.
    const FOREIGN_KEY: &'static str;
    /// Table name of the child model.
    const CHILD_TABLE: &'static str;
}

/// Declares a one-to-many relationship where the foreign key is on the child.
pub trait HasMany<Child> {
    /// Column name on the child's table holding the foreign key.
    const FOREIGN_KEY: &'static str;
    /// Table name of the child model.
    const CHILD_TABLE: &'static str;
}

/// Extracts the foreign key value pointing back to a parent.
pub trait ForeignKey<Parent> {
    /// Returns the foreign key value for this record.
    fn fk_value(&self) -> i64;
}

fn unique_ids(iter: impl Iterator<Item = i64>) -> Vec<Value> {
    let set: std::collections::HashSet<i64> = iter.collect();
    set.into_iter().map(Value::I64).collect()
}

/// Batch-loads related records in a single query to prevent N+1.
pub struct Related;

impl Related {
    /// Load parent records for a slice of children via their `BelongsTo` FK.
    ///
    /// Returns a `HashMap` keyed by the parent's `id`. Runs a single
    /// `SELECT * FROM parent_table WHERE id IN (...)` query.
    pub async fn load<C, P>(children: &[C], pool: &DbPool) -> Result<HashMap<i64, P>>
    where
        C: BelongsTo<P>,
        P: for<'r> FromRow<'r, DbRow> + HasId + Send + Unpin,
    {
        if children.is_empty() {
            return Ok(HashMap::new());
        }

        let ids = unique_ids(children.iter().map(|c| c.fk_value()));
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let parents = Select::from(C::PARENT_TABLE)
            .where_in("id", ids)
            .fetch_all::<P>(pool)
            .await?;

        Ok(parents.into_iter().map(|p| (p.id(), p)).collect())
    }

    /// Load child records for a slice of parents via their `HasMany` FK.
    ///
    /// Returns a `HashMap` keyed by the parent's `id`, each value is a
    /// `Vec` of matching children. Runs a single
    /// `SELECT * FROM child_table WHERE fk_column IN (...)` query.
    pub async fn load_many<P, C>(parents: &[P], pool: &DbPool) -> Result<HashMap<i64, Vec<C>>>
    where
        P: HasMany<C> + HasId,
        C: for<'r> FromRow<'r, DbRow> + ForeignKey<P> + Send + Unpin,
    {
        if parents.is_empty() {
            return Ok(HashMap::new());
        }

        let ids = unique_ids(parents.iter().map(|p| p.id()));
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let children = Select::from(P::CHILD_TABLE)
            .where_in(P::FOREIGN_KEY, ids)
            .fetch_all::<C>(pool)
            .await?;

        let mut map: HashMap<i64, Vec<C>> = HashMap::new();
        for child in children {
            map.entry(child.fk_value()).or_default().push(child);
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Post {
        id: i64,
        author_id: i64,
    }
    struct User {
        id: i64,
    }
    struct Comment {
        _id: i64,
        post_id: i64,
    }

    impl HasId for Post {
        fn id(&self) -> i64 {
            self.id
        }
    }
    impl HasId for User {
        fn id(&self) -> i64 {
            self.id
        }
    }

    impl BelongsTo<User> for Post {
        const FOREIGN_KEY: &'static str = "author_id";
        const PARENT_TABLE: &'static str = "users";
        fn fk_value(&self) -> i64 {
            self.author_id
        }
    }

    impl HasMany<Comment> for Post {
        const FOREIGN_KEY: &'static str = "post_id";
        const CHILD_TABLE: &'static str = "comments";
    }

    impl ForeignKey<Post> for Comment {
        fn fk_value(&self) -> i64 {
            self.post_id
        }
    }

    #[test]
    fn unique_ids_deduplicates() {
        let vals = unique_ids([1, 2, 2, 3, 1].into_iter());
        assert_eq!(vals.len(), 3);
    }

    #[test]
    fn trait_constants_accessible() {
        assert_eq!(<Post as BelongsTo<User>>::PARENT_TABLE, "users");
        assert_eq!(<Post as BelongsTo<User>>::FOREIGN_KEY, "author_id");
        assert_eq!(<Post as HasMany<Comment>>::CHILD_TABLE, "comments");
        assert_eq!(<Post as HasMany<Comment>>::FOREIGN_KEY, "post_id");
    }

    #[test]
    fn fk_value_extracts_correctly() {
        let post = Post {
            id: 1,
            author_id: 42,
        };
        assert_eq!(BelongsTo::<User>::fk_value(&post), 42);

        let comment = Comment { _id: 1, post_id: 7 };
        assert_eq!(ForeignKey::<Post>::fk_value(&comment), 7);
    }
}
