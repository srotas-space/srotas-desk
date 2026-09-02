use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use super::RepoError;

/// Value of everything currently in stock, priced at buy cost (what the
/// shop has actually paid for it), in paise.
/// One page of the items that have fallen below their own threshold —
/// what the Details screen's low-stock panel draws. Ordered by how far
/// below they are, so the most urgent restock is on page one rather than
/// wherever the alphabet happens to put it.
pub async fn low_stock_page(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
) -> Result<Vec<crate::models::Item>, RepoError> {
    let items = sqlx::query_as::<_, crate::models::Item>(
        "SELECT id, name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold, \
                description, location, (image IS NOT NULL) AS has_image, gst_rate_bp \
         FROM items \
         WHERE deleted = 0 AND stock_qty < low_stock_threshold \
         ORDER BY (stock_qty / NULLIF(low_stock_threshold, 0)) ASC, name \
         LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(items)
}

/// How many items are below their threshold — the figure the Home screen
/// badge shows, and the total behind the low-stock pagination.
pub async fn low_stock_count(pool: &SqlitePool) -> Result<i64, RepoError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM items WHERE deleted = 0 AND stock_qty < low_stock_threshold",
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

pub async fn current_stock_value_paise(pool: &SqlitePool) -> Result<i64, RepoError> {
    let value: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(stock_qty * buy_price_paise), 0.0) FROM items WHERE deleted = 0",
    )
    .fetch_one(pool)
    .await?;
    Ok(value.round() as i64)
}

/// Total profit over the filter, computed against each item's *current*
/// buy price. This is the simple MVP formula the spec calls for — it
/// doesn't track cost per purchase batch (FIFO/weighted-average), so if a
/// buy price changes after a sale, that sale's reported profit changes
/// too.
///
/// Summed in SQL rather than by loading every sale into memory: a shop
/// with years of history has hundreds of thousands of rows, and the
/// screen only ever shows the one total.
pub async fn total_profit_paise(
    pool: &SqlitePool,
    item_id: Option<i64>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<i64, RepoError> {
    // ROUND per row before summing, so the total matches what you get by
    // adding up the individual sales by hand.
    let total: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(ROUND((t.price_paise - i.buy_price_paise) * t.qty)), 0.0) \
         FROM transactions t \
         JOIN items i ON i.id = t.item_id \
         WHERE t.type = 'sell' AND t.deleted = 0 \
           AND (? IS NULL OR t.item_id = ?) \
           AND (? IS NULL OR t.timestamp >= ?) \
           AND (? IS NULL OR t.timestamp <= ?)",
    )
    .bind(item_id)
    .bind(item_id)
    .bind(from)
    .bind(from)
    .bind(to)
    .bind(to)
    .fetch_one(pool)
    .await?;

    Ok(total as i64)
}

/// How many transactions match a filter — the total behind the report's
/// pagination, so the screen can say "1-25 of 8,431" without holding
/// 8,431 rows.
pub async fn transaction_count(
    pool: &SqlitePool,
    item_id: Option<i64>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<i64, RepoError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM transactions t \
         WHERE t.deleted = 0 \
           AND (? IS NULL OR t.item_id = ?) \
           AND (? IS NULL OR t.timestamp >= ?) \
           AND (? IS NULL OR t.timestamp <= ?)",
    )
    .bind(item_id)
    .bind(item_id)
    .bind(from)
    .bind(from)
    .bind(to)
    .bind(to)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// A buy-or-sell row for the general transaction history view.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TransactionHistoryRow {
    pub item_name: String,
    pub kind: String,
    pub qty: f64,
    pub price_paise: i64,
    pub timestamp: DateTime<Utc>,
}

pub async fn transaction_history(
    pool: &SqlitePool,
    kind: Option<&str>,
    item_id: Option<i64>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<TransactionHistoryRow>, RepoError> {
    let rows = sqlx::query_as::<_, TransactionHistoryRow>(
        "SELECT \
             i.name AS item_name, \
             t.type AS kind, \
             t.qty AS qty, \
             t.price_paise AS price_paise, \
             t.timestamp AS timestamp \
         FROM transactions t \
         JOIN items i ON i.id = t.item_id \
         WHERE t.deleted = 0 \
           AND (? IS NULL OR t.type = ?) \
           AND (? IS NULL OR t.item_id = ?) \
           AND (? IS NULL OR t.timestamp >= ?) \
           AND (? IS NULL OR t.timestamp <= ?) \
         ORDER BY t.timestamp DESC \
         LIMIT ? OFFSET ?",
    )
    .bind(kind)
    .bind(kind)
    .bind(item_id)
    .bind(item_id)
    .bind(from)
    .bind(from)
    .bind(to)
    .bind(to)
    .bind(limit.unwrap_or(-1)) // SQLite treats a negative LIMIT as "no limit"
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(rows)
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

    /// `buy_price_paise` is what profit is measured against — see the note
    /// on `SaleProfitRow`.
    async fn seed_item(pool: &SqlitePool, name: &str, buy_paise: i64) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO items (name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold) \
             VALUES (?, ?, ?, 1000, 'piece', 5) RETURNING id",
        )
        .bind(name)
        .bind(buy_paise)
        .bind(buy_paise * 2)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn the_low_stock_page_is_bounded_and_ordered_by_urgency() {
        let pool = test_pool().await;
        // stock/threshold ratios: 0.1, 0.5, 0.9 — and one healthy item.
        for (name, stock, threshold) in
            [("Nearly out", 1.0, 10.0), ("Half", 5.0, 10.0), ("Nearly fine", 9.0, 10.0), ("Healthy", 99.0, 10.0)]
        {
            sqlx::query(
                "INSERT INTO items (name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold) \
                 VALUES (?, 100, 200, ?, 'piece', ?)",
            )
            .bind(name)
            .bind(stock)
            .bind(threshold)
            .execute(&pool)
            .await
            .unwrap();
        }

        assert_eq!(low_stock_count(&pool).await.unwrap(), 3, "the healthy item is excluded");

        let page = low_stock_page(&pool, 2, 0).await.unwrap();
        assert_eq!(page.len(), 2, "a page never exceeds its limit");
        assert_eq!(page[0].name, "Nearly out", "most urgent first");
        assert_eq!(page[1].name, "Half");

        let next = low_stock_page(&pool, 2, 2).await.unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].name, "Nearly fine");
    }

    #[tokio::test]
    async fn profit_is_sale_price_less_buy_price_across_every_sale() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 8000).await;

        crate::repo::record_sale(&pool, item, 2.0, 12000, Utc::now()).await.unwrap();
        crate::repo::record_sale(&pool, item, 3.0, 11000, Utc::now()).await.unwrap();

        // (12000-8000)*2 + (11000-8000)*3 = 8000 + 9000
        assert_eq!(total_profit_paise(&pool, None, None, None).await.unwrap(), 17_000);
    }

    #[tokio::test]
    async fn purchases_and_deleted_sales_are_left_out() {
        let pool = test_pool().await;
        let item = seed_item(&pool, "Item A", 8000).await;

        crate::repo::record_purchase(&pool, item, 5.0, 8000, Utc::now()).await.unwrap();
        crate::repo::record_sale(&pool, item, 1.0, 12000, Utc::now()).await.unwrap();
        crate::repo::record_sale(&pool, item, 1.0, 12000, Utc::now()).await.unwrap();
        sqlx::query("UPDATE transactions SET deleted = 1 WHERE type = 'sell' AND id = (SELECT MAX(id) FROM transactions)")
            .execute(&pool)
            .await
            .unwrap();

        // Only the one surviving sale counts, and the purchase never does.
        assert_eq!(total_profit_paise(&pool, None, None, None).await.unwrap(), 4_000);
    }

    #[tokio::test]
    async fn the_item_filter_narrows_the_total() {
        let pool = test_pool().await;
        let a = seed_item(&pool, "Item A", 8000).await;
        let b = seed_item(&pool, "Item B", 5000).await;

        crate::repo::record_sale(&pool, a, 1.0, 12000, Utc::now()).await.unwrap();
        crate::repo::record_sale(&pool, b, 1.0, 9000, Utc::now()).await.unwrap();

        assert_eq!(total_profit_paise(&pool, Some(a), None, None).await.unwrap(), 4_000);
        assert_eq!(total_profit_paise(&pool, Some(b), None, None).await.unwrap(), 4_000);
        assert_eq!(total_profit_paise(&pool, None, None, None).await.unwrap(), 8_000);
    }

    #[tokio::test]
    async fn a_period_with_no_sales_is_zero_not_an_error() {
        let pool = test_pool().await;
        assert_eq!(total_profit_paise(&pool, None, None, None).await.unwrap(), 0);
    }
}
