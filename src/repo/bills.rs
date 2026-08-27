use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use super::RepoError;
use crate::models::{BillDetail, BillLine, BillSummary};

#[derive(Debug, Clone)]
pub struct BillLineInput {
    pub item_id: i64,
    pub item_name: String,
    pub qty: f64,
    pub price_paise: i64,
    /// The GST rate to apply to this line, already resolved (item override,
    /// falling back to the shop default) by the caller — basis points,
    /// 1800 = 18.00%. Resolving it here rather than re-querying the item
    /// keeps this module a pure calculator over whatever the cart already
    /// has, same as `price_paise`.
    pub gst_rate_bp: i64,
}

/// One priced-and-taxed line, ready to insert — the taxable value already
/// has this line's share of the bill's flat discount subtracted out.
struct TaxedLine<'a> {
    input: &'a BillLineInput,
    line_total_paise: i64,
    cgst_paise: i64,
    sgst_paise: i64,
}

fn line_total(line: &BillLineInput) -> i64 {
    (line.price_paise as f64 * line.qty).round() as i64
}

/// Splits the bill's flat discount across lines in proportion to each
/// line's share of the subtotal, then computes CGST/SGST on what's left of
/// each line — so a line taxed at a different rate contributes the right
/// amount of tax even after a bill-wide discount. The last line absorbs
/// whatever rounding remainder is left, so the allocated discounts always
/// sum to exactly `discount_paise`.
fn tax_lines(lines: &[BillLineInput], discount_paise: i64) -> Vec<TaxedLine<'_>> {
    let totals: Vec<i64> = lines.iter().map(line_total).collect();
    let subtotal: i64 = totals.iter().sum();

    let mut allocated_discount = 0i64;
    let mut taxed = Vec::with_capacity(lines.len());
    for (i, (input, &total)) in lines.iter().zip(&totals).enumerate() {
        let discount_share = if subtotal == 0 {
            0
        } else if i == lines.len() - 1 {
            discount_paise - allocated_discount
        } else {
            let share = (discount_paise as i128 * total as i128 / subtotal as i128) as i64;
            allocated_discount += share;
            share
        };

        let taxable = (total - discount_share).max(0);
        // CGST and SGST are each half of the full rate — round-half-up on
        // the combined tax first, then split, so the two never differ by a
        // stray paisa from being rounded independently.
        let total_tax = ((taxable as i128 * input.gst_rate_bp as i128 + 5_000) / 10_000) as i64;
        let cgst_paise = total_tax / 2;
        let sgst_paise = total_tax - cgst_paise;

        taxed.push(TaxedLine { input, line_total_paise: total, cgst_paise, sgst_paise });
    }
    taxed
}

/// Atomically decrements stock for one bill line, the same way
/// `record_sale` does — one UPDATE with the stock check built into the
/// WHERE clause, so two concurrent bills can't both oversell the last unit.
async fn adjust_stock(tx: &mut sqlx::SqliteConnection, item_id: i64, delta: f64) -> Result<(), RepoError> {
    if delta == 0.0 {
        return Ok(());
    }
    if delta > 0.0 {
        // Selling more than before this edit — needs the same
        // sufficient-stock guard as a fresh sale.
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
        // Selling less than before (or removing a line entirely) — restores stock, can't fail.
        sqlx::query("UPDATE items SET stock_qty = stock_qty - ? WHERE id = ? AND deleted = 0")
            .bind(delta) // delta is negative, so this adds back
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}

async fn insert_bill_items(
    tx: &mut sqlx::SqliteConnection,
    bill_id: i64,
    taxed: &[TaxedLine<'_>],
) -> Result<(), RepoError> {
    for line in taxed {
        sqlx::query(
            "INSERT INTO bill_items (bill_id, item_id, item_name, qty, price_paise, line_total_paise, gst_rate_bp, cgst_paise, sgst_paise) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(bill_id)
        .bind(line.input.item_id)
        .bind(&line.input.item_name)
        .bind(line.input.qty)
        .bind(line.input.price_paise)
        .bind(line.line_total_paise)
        .bind(line.input.gst_rate_bp)
        .bind(line.cgst_paise)
        .bind(line.sgst_paise)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

pub async fn create_bill(pool: &SqlitePool, lines: &[BillLineInput], discount_paise: i64) -> Result<i64, RepoError> {
    if lines.is_empty() {
        return Err(RepoError::InvalidQty);
    }

    let taxed = tax_lines(lines, discount_paise);
    let subtotal_paise: i64 = lines.iter().map(line_total).sum();
    let cgst_paise: i64 = taxed.iter().map(|l| l.cgst_paise).sum();
    let sgst_paise: i64 = taxed.iter().map(|l| l.sgst_paise).sum();
    let total_paise = (subtotal_paise - discount_paise).max(0) + cgst_paise + sgst_paise;

    let mut tx = pool.begin().await?;

    for line in lines {
        adjust_stock(&mut tx, line.item_id, line.qty).await?;
    }

    let bill_id: i64 = sqlx::query_scalar(
        "INSERT INTO bills (subtotal_paise, discount_paise, cgst_paise, sgst_paise, total_paise, timestamp) \
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(subtotal_paise)
    .bind(discount_paise)
    .bind(cgst_paise)
    .bind(sgst_paise)
    .bind(total_paise)
    .bind(Utc::now())
    .fetch_one(&mut *tx)
    .await?;

    insert_bill_items(&mut tx, bill_id, &taxed).await?;

    tx.commit().await?;
    Ok(bill_id)
}

/// Replaces a bill's line items and discount, reconciling stock by the
/// per-item quantity delta (old vs. new) rather than fully reversing and
/// redoing — so an edit that only tweaks the discount touches no stock at
/// all, and one that changes a quantity only moves the difference.
pub async fn edit_bill(pool: &SqlitePool, id: i64, lines: &[BillLineInput], discount_paise: i64) -> Result<(), RepoError> {
    if lines.is_empty() {
        return Err(RepoError::InvalidQty);
    }

    let mut tx = pool.begin().await?;

    let existing: Vec<(i64, f64)> = sqlx::query_as("SELECT item_id, qty FROM bill_items WHERE bill_id = ?")
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
    if existing.is_empty() {
        return Err(RepoError::ItemNotFound);
    }

    let mut old_qty_by_item: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    for (item_id, qty) in existing {
        *old_qty_by_item.entry(item_id).or_insert(0.0) += qty;
    }
    let mut new_qty_by_item: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    for line in lines {
        *new_qty_by_item.entry(line.item_id).or_insert(0.0) += line.qty;
    }

    let mut all_item_ids: Vec<i64> = old_qty_by_item.keys().copied().collect();
    for id in new_qty_by_item.keys() {
        if !all_item_ids.contains(id) {
            all_item_ids.push(*id);
        }
    }

    for item_id in all_item_ids {
        let old_qty = old_qty_by_item.get(&item_id).copied().unwrap_or(0.0);
        let new_qty = new_qty_by_item.get(&item_id).copied().unwrap_or(0.0);
        adjust_stock(&mut tx, item_id, new_qty - old_qty).await?;
    }

    sqlx::query("DELETE FROM bill_items WHERE bill_id = ?").bind(id).execute(&mut *tx).await?;
    let taxed = tax_lines(lines, discount_paise);
    insert_bill_items(&mut tx, id, &taxed).await?;

    let subtotal_paise: i64 = lines.iter().map(line_total).sum();
    let cgst_paise: i64 = taxed.iter().map(|l| l.cgst_paise).sum();
    let sgst_paise: i64 = taxed.iter().map(|l| l.sgst_paise).sum();
    let total_paise = (subtotal_paise - discount_paise).max(0) + cgst_paise + sgst_paise;

    let affected = sqlx::query(
        "UPDATE bills SET subtotal_paise = ?, discount_paise = ?, cgst_paise = ?, sgst_paise = ?, total_paise = ? \
         WHERE id = ? AND deleted = 0",
    )
    .bind(subtotal_paise)
    .bind(discount_paise)
    .bind(cgst_paise)
    .bind(sgst_paise)
    .bind(total_paise)
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(RepoError::ItemNotFound);
    }

    tx.commit().await?;
    Ok(())
}

/// Soft delete — hides the bill from history without reversing its stock
/// effect. Deleting a record isn't the same as un-selling the goods.
pub async fn delete_bill(pool: &SqlitePool, id: i64) -> Result<(), RepoError> {
    let affected = sqlx::query("UPDATE bills SET deleted = 1 WHERE id = ? AND deleted = 0")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(RepoError::ItemNotFound);
    }
    Ok(())
}

pub async fn list_bills(pool: &SqlitePool, page: i64, page_size: i64) -> Result<(Vec<BillSummary>, i64), RepoError> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bills WHERE deleted = 0").fetch_one(pool).await?;

    let bills = sqlx::query_as::<_, BillSummary>(
        "SELECT b.id, b.total_paise, b.timestamp, \
                (SELECT COUNT(*) FROM bill_items bi WHERE bi.bill_id = b.id) AS item_count \
         FROM bills b \
         WHERE b.deleted = 0 \
         ORDER BY b.timestamp DESC \
         LIMIT ? OFFSET ?",
    )
    .bind(page_size)
    .bind(page * page_size)
    .fetch_all(pool)
    .await?;

    Ok((bills, total))
}

pub async fn get_bill(pool: &SqlitePool, id: i64) -> Result<BillDetail, RepoError> {
    let header: Option<(i64, i64, i64, i64, i64, DateTime<Utc>)> = sqlx::query_as(
        "SELECT subtotal_paise, discount_paise, cgst_paise, sgst_paise, total_paise, timestamp FROM bills WHERE id = ? AND deleted = 0",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some((subtotal_paise, discount_paise, cgst_paise, sgst_paise, total_paise, timestamp)) = header else {
        return Err(RepoError::ItemNotFound);
    };

    let lines = sqlx::query_as::<_, BillLine>(
        "SELECT id, item_id, item_name, qty, price_paise, line_total_paise, gst_rate_bp, cgst_paise, sgst_paise \
         FROM bill_items WHERE bill_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    Ok(BillDetail { id, subtotal_paise, discount_paise, cgst_paise, sgst_paise, total_paise, timestamp, lines })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::str::FromStr;

    async fn test_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        // A single connection, so every query in the test sees the same
        // in-memory database instead of each getting its own empty one.
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn seed_item(pool: &SqlitePool, name: &str, stock: f64, sell_price_paise: i64) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO items (name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold) \
             VALUES (?, 5000, ?, ?, 'piece', 5) RETURNING id",
        )
        .bind(name)
        .bind(sell_price_paise)
        .bind(stock)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn stock_of(pool: &SqlitePool, item_id: i64) -> f64 {
        sqlx::query_scalar("SELECT stock_qty FROM items WHERE id = ?").bind(item_id).fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn create_bill_decrements_stock_and_computes_total() {
        let pool = test_pool().await;
        let item_a = seed_item(&pool, "Item A", 10.0, 10000).await;
        let item_b = seed_item(&pool, "Item B", 10.0, 5000).await;

        let lines = vec![
            BillLineInput { item_id: item_a, item_name: "Item A".into(), qty: 2.0, price_paise: 10000, gst_rate_bp: 0 },
            BillLineInput { item_id: item_b, item_name: "Item B".into(), qty: 3.0, price_paise: 5000, gst_rate_bp: 0 },
        ];
        let bill_id = create_bill(&pool, &lines, 1000).await.unwrap();

        // subtotal = 2*10000 + 3*5000 = 35000; discount 1000 -> total 34000
        let detail = get_bill(&pool, bill_id).await.unwrap();
        assert_eq!(detail.subtotal_paise, 35000);
        assert_eq!(detail.discount_paise, 1000);
        assert_eq!(detail.total_paise, 34000);
        assert_eq!(detail.lines.len(), 2);

        assert_eq!(stock_of(&pool, item_a).await, 8.0);
        assert_eq!(stock_of(&pool, item_b).await, 7.0);
    }

    #[tokio::test]
    async fn create_bill_computes_cgst_and_sgst_split_per_line_after_discount() {
        let pool = test_pool().await;
        let item_a = seed_item(&pool, "Item A", 10.0, 10000).await; // 18% GST
        let item_b = seed_item(&pool, "Item B", 10.0, 5000).await; // 12% GST

        let lines = vec![
            BillLineInput { item_id: item_a, item_name: "Item A".into(), qty: 2.0, price_paise: 10000, gst_rate_bp: 1800 },
            BillLineInput { item_id: item_b, item_name: "Item B".into(), qty: 3.0, price_paise: 5000, gst_rate_bp: 1200 },
        ];
        // subtotal = 20000 + 15000 = 35000; discount 3500 (10%) prorated
        // 2000/1500 across the two lines -> taxable 18000 and 13500.
        let bill_id = create_bill(&pool, &lines, 3500).await.unwrap();

        let detail = get_bill(&pool, bill_id).await.unwrap();
        assert_eq!(detail.subtotal_paise, 35000);
        assert_eq!(detail.discount_paise, 3500);

        let line_a = detail.lines.iter().find(|l| l.item_id == item_a).unwrap();
        let line_b = detail.lines.iter().find(|l| l.item_id == item_b).unwrap();
        // line A: taxable 18000 * 18% = 3240 total tax -> 1620/1620 CGST/SGST
        assert_eq!(line_a.cgst_paise, 1620);
        assert_eq!(line_a.sgst_paise, 1620);
        // line B: taxable 13500 * 12% = 1620 total tax -> 810/810 CGST/SGST
        assert_eq!(line_b.cgst_paise, 810);
        assert_eq!(line_b.sgst_paise, 810);

        assert_eq!(detail.cgst_paise, 1620 + 810);
        assert_eq!(detail.sgst_paise, 1620 + 810);
        // total = (35000 - 3500) + 2430 + 2430 = 36360
        assert_eq!(detail.total_paise, 36360);
    }

    #[tokio::test]
    async fn create_bill_rejects_overselling_any_line() {
        let pool = test_pool().await;
        let item_a = seed_item(&pool, "Item A", 1.0, 10000).await;

        let lines = vec![BillLineInput { item_id: item_a, item_name: "Item A".into(), qty: 5.0, price_paise: 10000, gst_rate_bp: 0 }];
        let result = create_bill(&pool, &lines, 0).await;
        assert!(result.is_err());
        // stock must be unchanged since the whole bill rolled back
        assert_eq!(stock_of(&pool, item_a).await, 1.0);
    }

    #[tokio::test]
    async fn edit_bill_reconciles_stock_by_delta() {
        let pool = test_pool().await;
        let item_a = seed_item(&pool, "Item A", 10.0, 10000).await;

        let lines = vec![BillLineInput { item_id: item_a, item_name: "Item A".into(), qty: 2.0, price_paise: 10000, gst_rate_bp: 0 }];
        let bill_id = create_bill(&pool, &lines, 0).await.unwrap();
        assert_eq!(stock_of(&pool, item_a).await, 8.0);

        // Increase qty from 2 to 5 — should take 3 more units of stock.
        let new_lines = vec![BillLineInput { item_id: item_a, item_name: "Item A".into(), qty: 5.0, price_paise: 10000, gst_rate_bp: 0 }];
        edit_bill(&pool, bill_id, &new_lines, 0).await.unwrap();
        assert_eq!(stock_of(&pool, item_a).await, 5.0);

        let detail = get_bill(&pool, bill_id).await.unwrap();
        assert_eq!(detail.subtotal_paise, 50000);
        assert_eq!(detail.lines.len(), 1);
        assert_eq!(detail.lines[0].qty, 5.0);

        // Decrease qty from 5 back to 1 — should return 4 units of stock.
        let smaller_lines = vec![BillLineInput { item_id: item_a, item_name: "Item A".into(), qty: 1.0, price_paise: 10000, gst_rate_bp: 0 }];
        edit_bill(&pool, bill_id, &smaller_lines, 0).await.unwrap();
        assert_eq!(stock_of(&pool, item_a).await, 9.0);
    }

    #[tokio::test]
    async fn delete_bill_soft_deletes_without_restocking() {
        let pool = test_pool().await;
        let item_a = seed_item(&pool, "Item A", 10.0, 10000).await;
        let lines = vec![BillLineInput { item_id: item_a, item_name: "Item A".into(), qty: 2.0, price_paise: 10000, gst_rate_bp: 0 }];
        let bill_id = create_bill(&pool, &lines, 0).await.unwrap();

        delete_bill(&pool, bill_id).await.unwrap();

        assert_eq!(stock_of(&pool, item_a).await, 8.0, "delete must not restock");
        assert!(get_bill(&pool, bill_id).await.is_err(), "deleted bill should no longer be fetchable");

        let (rows, total) = list_bills(&pool, 0, 10).await.unwrap();
        assert_eq!(total, 0);
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn list_bills_paginates() {
        let pool = test_pool().await;
        let item_a = seed_item(&pool, "Item A", 1000.0, 10000).await;
        for _ in 0..15 {
            let lines = vec![BillLineInput { item_id: item_a, item_name: "Item A".into(), qty: 1.0, price_paise: 10000, gst_rate_bp: 0 }];
            create_bill(&pool, &lines, 0).await.unwrap();
        }

        let (page0, total) = list_bills(&pool, 0, 10).await.unwrap();
        assert_eq!(total, 15);
        assert_eq!(page0.len(), 10);

        let (page1, _) = list_bills(&pool, 1, 10).await.unwrap();
        assert_eq!(page1.len(), 5);
    }
}
