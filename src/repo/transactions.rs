use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use super::RepoError;
use crate::models::Transaction;

pub async fn record_purchase(
    pool: &SqlitePool,
    item_id: i64,
    qty: f64,
    price_paise: i64,
    timestamp: DateTime<Utc>,
) -> Result<Transaction, RepoError> {
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

    let inserted = sqlx::query_as::<_, Transaction>(
        "INSERT INTO transactions (item_id, type, qty, price_paise, timestamp) \
         VALUES (?, 'buy', ?, ?, ?) \
         RETURNING id, item_id, type AS kind, qty, price_paise, timestamp",
    )
    .bind(item_id)
    .bind(qty)
    .bind(price_paise)
    .bind(timestamp)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(inserted)
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
) -> Result<Transaction, RepoError> {
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

    let inserted = sqlx::query_as::<_, Transaction>(
        "INSERT INTO transactions (item_id, type, qty, price_paise, timestamp) \
         VALUES (?, 'sell', ?, ?, ?) \
         RETURNING id, item_id, type AS kind, qty, price_paise, timestamp",
    )
    .bind(item_id)
    .bind(qty)
    .bind(price_paise)
    .bind(timestamp)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(inserted)
}
