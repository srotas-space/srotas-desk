use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use super::{RepoError, TransactionHistoryRow};
use crate::models::Transaction;

/// Atomically adjusts one item's stock by `delta` — positive means "sell
/// this many more" (guarded by a stock check), negative means "restore
/// this many" (can't fail). Same shape as `repo::bills`'s helper of the
/// same name; kept as its own small copy here rather than shared, same
/// reasoning as the GST calculation duplicated across this app's modules.
async fn adjust_stock(tx: &mut sqlx::SqliteConnection, item_id: i64, delta: f64) -> Result<(), RepoError> {
    if delta == 0.0 {
        return Ok(());
    }
    if delta > 0.0 {
        let affected = sqlx::query("UPDATE items SET stock_qty = stock_qty - ? WHERE id = ? AND deleted = 0 AND stock_qty >= ?")
            .bind(delta)
            .bind(item_id)
            .bind(delta)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if affected == 0 {
            let current: Option<(f64, bool)> = sqlx::query_as("SELECT stock_qty, deleted FROM items WHERE id = ?")
                .bind(item_id)
                .fetch_optional(&mut *tx)
                .await?;
            return match current {
                None | Some((_, true)) => Err(RepoError::ItemNotFound),
                Some((available, false)) => Err(RepoError::InsufficientStock { available, requested: delta }),
            };
        }
    } else {
        sqlx::query("UPDATE items SET stock_qty = stock_qty - ? WHERE id = ? AND deleted = 0")
            .bind(delta)
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}

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

/// Fetches one sale (not a purchase — see the `type = 'sell'` guard) for
/// the View/Edit/Print/Download actions on the Sales history row.
pub async fn get_sale(pool: &SqlitePool, id: i64) -> Result<TransactionHistoryRow, RepoError> {
    let row = sqlx::query_as::<_, TransactionHistoryRow>(
        "SELECT t.id AS id, t.item_id AS item_id, i.name AS item_name, t.type AS kind, t.qty AS qty, t.price_paise AS price_paise, t.timestamp AS timestamp \
         FROM transactions t JOIN items i ON i.id = t.item_id \
         WHERE t.id = ? AND t.type = 'sell' AND t.deleted = 0",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.ok_or(RepoError::ItemNotFound)
}

/// Corrects a past sale's item/quantity/price, reconciling stock by
/// whatever changed — same reasoning as `repo::bills::edit_bill`: an edit
/// fixes the record *and* the real stock effect it had, unlike a delete.
/// Handles the item being changed too (rare, but "picked the wrong item"
/// happens): the old item is fully restocked and the new one fully
/// decremented, rather than trying to diff between two different items.
pub async fn edit_sale(
    pool: &SqlitePool,
    id: i64,
    new_item_id: i64,
    new_qty: f64,
    new_price_paise: i64,
) -> Result<Transaction, RepoError> {
    if new_qty <= 0.0 {
        return Err(RepoError::InvalidQty);
    }

    let mut tx = pool.begin().await?;

    let existing: Option<(i64, f64)> =
        sqlx::query_as("SELECT item_id, qty FROM transactions WHERE id = ? AND type = 'sell' AND deleted = 0")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((old_item_id, old_qty)) = existing else {
        return Err(RepoError::ItemNotFound);
    };

    if old_item_id == new_item_id {
        adjust_stock(&mut tx, new_item_id, new_qty - old_qty).await?;
    } else {
        adjust_stock(&mut tx, old_item_id, -old_qty).await?;
        adjust_stock(&mut tx, new_item_id, new_qty).await?;
    }

    let updated = sqlx::query_as::<_, Transaction>(
        "UPDATE transactions SET item_id = ?, qty = ?, price_paise = ? WHERE id = ? \
         RETURNING id, item_id, type AS kind, qty, price_paise, timestamp",
    )
    .bind(new_item_id)
    .bind(new_qty)
    .bind(new_price_paise)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(updated)
}

/// Soft delete — hides the sale from history without restocking. Voiding
/// a record isn't the same as reversing the real-world sale, same
/// reasoning as `repo::bills::delete_bill`.
pub async fn delete_sale(pool: &SqlitePool, id: i64) -> Result<(), RepoError> {
    let affected = sqlx::query("UPDATE transactions SET deleted = 1 WHERE id = ? AND type = 'sell' AND deleted = 0")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(RepoError::ItemNotFound);
    }
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

    async fn stock_of(pool: &SqlitePool, item_id: i64) -> f64 {
        sqlx::query_scalar("SELECT stock_qty FROM items WHERE id = ?").bind(item_id).fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn get_sale_returns_the_recorded_sale() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 10.0).await;
        let sale = record_sale(&pool, item, 2.0, 10000, Utc::now()).await.unwrap();

        let fetched = get_sale(&pool, sale.id).await.unwrap();
        assert_eq!(fetched.item_id, item);
        assert_eq!(fetched.qty, 2.0);
        assert_eq!(fetched.price_paise, 10000);
    }

    #[tokio::test]
    async fn edit_sale_on_the_same_item_adjusts_stock_by_the_delta() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 10.0).await;
        let sale = record_sale(&pool, item, 2.0, 10000, Utc::now()).await.unwrap();
        assert_eq!(stock_of(&pool, item).await, 8.0);

        // Correcting the quantity from 2 to 5 should take 3 more units.
        edit_sale(&pool, sale.id, item, 5.0, 10000).await.unwrap();
        assert_eq!(stock_of(&pool, item).await, 5.0);

        let fetched = get_sale(&pool, sale.id).await.unwrap();
        assert_eq!(fetched.qty, 5.0);

        // Correcting it back down to 1 should return 4 units.
        edit_sale(&pool, sale.id, item, 1.0, 10000).await.unwrap();
        assert_eq!(stock_of(&pool, item).await, 9.0);
    }

    #[tokio::test]
    async fn edit_sale_rejects_insufficient_stock_and_leaves_everything_unchanged() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 10.0).await;
        let sale = record_sale(&pool, item, 2.0, 10000, Utc::now()).await.unwrap();

        let result = edit_sale(&pool, sale.id, item, 999.0, 10000).await;
        assert!(result.is_err());
        assert_eq!(stock_of(&pool, item).await, 8.0, "a rejected edit must not partially adjust stock");

        let fetched = get_sale(&pool, sale.id).await.unwrap();
        assert_eq!(fetched.qty, 2.0, "the original sale must be untouched");
    }

    #[tokio::test]
    async fn edit_sale_switching_items_restocks_the_old_one_and_decrements_the_new_one() {
        let pool = test_pool().await;
        let item_a = seed_item(&pool, "Item A", 10.0).await;
        let item_b = seed_item(&pool, "Item B", 10.0).await;
        let sale = record_sale(&pool, item_a, 2.0, 10000, Utc::now()).await.unwrap();
        assert_eq!(stock_of(&pool, item_a).await, 8.0);

        edit_sale(&pool, sale.id, item_b, 3.0, 10000).await.unwrap();
        assert_eq!(stock_of(&pool, item_a).await, 10.0, "old item should be fully restocked");
        assert_eq!(stock_of(&pool, item_b).await, 7.0, "new item should be decremented by the new quantity");

        let fetched = get_sale(&pool, sale.id).await.unwrap();
        assert_eq!(fetched.item_id, item_b);
        assert_eq!(fetched.qty, 3.0);
    }

    #[tokio::test]
    async fn delete_sale_soft_deletes_without_restocking() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 10.0).await;
        let sale = record_sale(&pool, item, 2.0, 10000, Utc::now()).await.unwrap();

        delete_sale(&pool, sale.id).await.unwrap();

        assert_eq!(stock_of(&pool, item).await, 8.0, "delete must not restock");
        assert!(get_sale(&pool, sale.id).await.is_err(), "a deleted sale should no longer be fetchable");
    }

    #[tokio::test]
    async fn edit_and_delete_do_not_affect_purchases() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 10.0).await;
        let purchase = record_purchase(&pool, item, 5.0, 8000, Utc::now()).await.unwrap();

        assert!(edit_sale(&pool, purchase.id, item, 1.0, 8000).await.is_err());
        assert!(delete_sale(&pool, purchase.id).await.is_err());
        assert_eq!(stock_of(&pool, item).await, 15.0, "a purchase-type row must be untouched");
    }
}
