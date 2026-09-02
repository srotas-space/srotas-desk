use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use super::RepoError;

pub async fn record_purchase(
    pool: &SqlitePool,
    item_id: i64,
    qty: f64,
    price_paise: i64,
    timestamp: DateTime<Utc>,
) -> Result<(), RepoError> {
    if qty <= 0.0 {
        return Err(RepoError::InvalidQty);
    }

    let mut tx = pool.begin().await?;

    let affected = sqlx::query("UPDATE items SET stock_qty = stock_qty + ? WHERE id = ? AND deleted = 0")
        .bind(qty)
        .bind(item_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(RepoError::ItemNotFound);
    }

    sqlx::query(
        "INSERT INTO transactions (item_id, type, qty, price_paise, timestamp) \
         VALUES (?, 'buy', ?, ?, ?)",
    )
    .bind(item_id)
    .bind(qty)
    .bind(price_paise)
    .bind(timestamp)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Records a sale, decrementing stock. The stock check and the decrement
/// happen in one atomic UPDATE (`stock_qty >= ?`) rather than a separate
/// "read stock, check, then write" — otherwise two sales submitted at
/// nearly the same instant could both read "1 in stock" and both succeed,
/// selling the same last unit twice.
pub async fn record_sale(
    pool: &SqlitePool,
    item_id: i64,
    qty: f64,
    price_paise: i64,
    timestamp: DateTime<Utc>,
) -> Result<(), RepoError> {
    if qty <= 0.0 {
        return Err(RepoError::InvalidQty);
    }

    let mut tx = pool.begin().await?;

    let affected = sqlx::query(
        "UPDATE items SET stock_qty = stock_qty - ? \
         WHERE id = ? AND deleted = 0 AND stock_qty >= ?",
    )
    .bind(qty)
    .bind(item_id)
    .bind(qty)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if affected == 0 {
        // The UPDATE matched nothing — find out whether that's because the
        // item doesn't exist/is deleted, or because stock was too low, so
        // the caller gets a useful error either way.
        let current: Option<(f64, bool)> =
            sqlx::query_as("SELECT stock_qty, deleted FROM items WHERE id = ?")
                .bind(item_id)
                .fetch_optional(&mut *tx)
                .await?;

        return match current {
            None => Err(RepoError::ItemNotFound),
            Some((_, true)) => Err(RepoError::ItemNotFound),
            Some((available, false)) => Err(RepoError::InsufficientStock {
                available,
                requested: qty,
            }),
        };
    }

    sqlx::query(
        "INSERT INTO transactions (item_id, type, qty, price_paise, timestamp) \
         VALUES (?, 'sell', ?, ?, ?)",
    )
    .bind(item_id)
    .bind(qty)
    .bind(price_paise)
    .bind(timestamp)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn test_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn seed_item(pool: &SqlitePool, name: &str, stock: f64) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO items (name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold) \
             VALUES (?, 5000, 10000, ?, 'piece', 5) RETURNING id",
        )
        .bind(name)
        .bind(stock)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// The writers return unit, so the recorded row is checked in the
    /// database rather than in their return value.
    async fn kind_of_only_transaction(pool: &SqlitePool) -> String {
        sqlx::query_scalar("SELECT type FROM transactions").fetch_one(pool).await.unwrap()
    }

    async fn stock_of(pool: &SqlitePool, item_id: i64) -> f64 {
        sqlx::query_scalar("SELECT stock_qty FROM items WHERE id = ?").bind(item_id).fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn record_purchase_adds_to_stock() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 10.0).await;

        record_purchase(&pool, item, 4.0, 8000, Utc::now()).await.unwrap();

        assert_eq!(kind_of_only_transaction(&pool).await, "buy");
        assert_eq!(stock_of(&pool, item).await, 14.0);
    }

    #[tokio::test]
    async fn record_sale_takes_stock_away() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 10.0).await;

        record_sale(&pool, item, 3.0, 12000, Utc::now()).await.unwrap();

        assert_eq!(kind_of_only_transaction(&pool).await, "sell");
        assert_eq!(stock_of(&pool, item).await, 7.0);
    }

    #[tokio::test]
    async fn record_sale_refuses_to_oversell_and_leaves_stock_alone() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 2.0).await;

        let result = record_sale(&pool, item, 5.0, 12000, Utc::now()).await;

        assert!(matches!(result, Err(RepoError::InsufficientStock { available, requested }) if available == 2.0 && requested == 5.0));
        assert_eq!(stock_of(&pool, item).await, 2.0, "a rejected sale must not move stock");
    }

    #[tokio::test]
    async fn selling_the_exact_remaining_stock_is_allowed() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 2.0).await;

        record_sale(&pool, item, 2.0, 12000, Utc::now()).await.unwrap();

        assert_eq!(stock_of(&pool, item).await, 0.0);
    }

    #[tokio::test]
    async fn a_non_positive_quantity_is_rejected() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 10.0).await;

        assert!(matches!(record_sale(&pool, item, 0.0, 12000, Utc::now()).await, Err(RepoError::InvalidQty)));
        assert!(matches!(record_sale(&pool, item, -1.0, 12000, Utc::now()).await, Err(RepoError::InvalidQty)));
        assert_eq!(stock_of(&pool, item).await, 10.0);
    }

    #[tokio::test]
    async fn selling_an_unknown_item_is_an_error() {
        let pool = test_pool().await;

        assert!(matches!(record_sale(&pool, 4242, 1.0, 12000, Utc::now()).await, Err(RepoError::ItemNotFound)));
    }
}
