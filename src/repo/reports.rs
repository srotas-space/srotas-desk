use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use super::RepoError;

/// Value of everything currently in stock, priced at buy cost (what the
/// shop has actually paid for it), in paise.
pub async fn current_stock_value_paise(pool: &SqlitePool) -> Result<i64, RepoError> {
    let value: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(stock_qty * buy_price_paise), 0.0) FROM items WHERE deleted = 0",
    )
    .fetch_one(pool)
    .await?;
    Ok(value.round() as i64)
}

/// One sale, with its profit computed against the item's *current* buy
/// price. This is the simple MVP formula the spec calls for — it doesn't
/// track cost per purchase batch (FIFO/weighted-average), so if buy price
/// changes after a sale, that sale's reported profit changes too.
#[derive(Debug, Clone)]
pub struct SaleReportRow {
    pub transaction_id: i64,
    pub item_id: i64,
    pub item_name: String,
    pub qty: f64,
    pub sell_price_paise: i64,
    pub buy_price_paise: i64,
    pub profit_paise: i64,
    pub timestamp: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct RawSaleRow {
    transaction_id: i64,
    item_id: i64,
    item_name: String,
    qty: f64,
    sell_price_paise: i64,
    buy_price_paise: i64,
    timestamp: DateTime<Utc>,
}

pub async fn sales_report(
    pool: &SqlitePool,
    item_id: Option<i64>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<Vec<SaleReportRow>, RepoError> {
    let rows = sqlx::query_as::<_, RawSaleRow>(
        "SELECT \
             t.id AS transaction_id, \
             t.item_id AS item_id, \
             i.name AS item_name, \
             t.qty AS qty, \
             t.price_paise AS sell_price_paise, \
             i.buy_price_paise AS buy_price_paise, \
             t.timestamp AS timestamp \
         FROM transactions t \
         JOIN items i ON i.id = t.item_id \
         WHERE t.type = 'sell' AND t.deleted = 0 \
           AND (? IS NULL OR t.item_id = ?) \
           AND (? IS NULL OR t.timestamp >= ?) \
           AND (? IS NULL OR t.timestamp <= ?) \
         ORDER BY t.timestamp DESC",
    )
    .bind(item_id)
    .bind(item_id)
    .bind(from)
    .bind(from)
    .bind(to)
    .bind(to)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let profit_paise =
                ((r.sell_price_paise - r.buy_price_paise) as f64 * r.qty).round() as i64;
            SaleReportRow {
                transaction_id: r.transaction_id,
                item_id: r.item_id,
                item_name: r.item_name,
                qty: r.qty,
                sell_price_paise: r.sell_price_paise,
                buy_price_paise: r.buy_price_paise,
                profit_paise,
                timestamp: r.timestamp,
            }
        })
        .collect())
}

pub async fn total_profit_paise(
    pool: &SqlitePool,
    item_id: Option<i64>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<i64, RepoError> {
    let rows = sales_report(pool, item_id, from, to).await?;
    Ok(rows.iter().map(|r| r.profit_paise).sum())
}

/// A buy-or-sell row for the general transaction history view.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TransactionHistoryRow {
    pub id: i64,
    pub item_id: i64,
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
) -> Result<Vec<TransactionHistoryRow>, RepoError> {
    let rows = sqlx::query_as::<_, TransactionHistoryRow>(
        "SELECT \
             t.id AS id, \
             t.item_id AS item_id, \
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
         LIMIT ?",
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
    .fetch_all(pool)
    .await?;

    Ok(rows)
}
