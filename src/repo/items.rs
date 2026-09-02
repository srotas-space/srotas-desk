use sqlx::SqlitePool;

use super::RepoError;
use crate::models::{Item, Unit};

/// The column list every `Item` query selects (or `RETURNING`s). A macro
/// rather than a `const` so the queries stay `concat!`ed string literals —
/// sqlx refuses a runtime-built query unless it is explicitly waved
/// through as injection-audited, and there is no reason to give up that
/// check to avoid retyping a column list.
macro_rules! item_columns {
    () => {
        "id, name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold, \
         description, location, (image IS NOT NULL) AS has_image, gst_rate_bp"
    };
}

/// Case-insensitive duplicate-name check, used before every insert/update
/// so the shopkeeper gets a friendly error instead of a raw SQL constraint
/// failure. `exclude_id` lets an edit ignore the row being edited itself.
async fn name_taken(pool: &SqlitePool, name: &str, exclude_id: Option<i64>) -> Result<bool, RepoError> {
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS( \
             SELECT 1 FROM items \
             WHERE deleted = 0 AND name = ? COLLATE NOCASE AND (? IS NULL OR id != ?) \
         )",
    )
    .bind(name)
    .bind(exclude_id)
    .bind(exclude_id)
    .fetch_one(pool)
    .await?;
    Ok(taken)
}

pub async fn add_item(
    pool: &SqlitePool,
    name: &str,
    buy_price_paise: i64,
    sell_price_paise: i64,
    stock_qty: f64,
    unit: Unit,
    low_stock_threshold: f64,
    description: &str,
    location: &str,
    image: Option<&[u8]>,
    gst_rate_bp: Option<i64>,
) -> Result<Item, RepoError> {
    if name_taken(pool, name, None).await? {
        return Err(RepoError::DuplicateItemName { name: name.to_string() });
    }

    let item = sqlx::query_as::<_, Item>(concat!(
        "INSERT INTO items (name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold, description, location, image, gst_rate_bp) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         RETURNING ", item_columns!()
    ))
    .bind(name)
    .bind(buy_price_paise)
    .bind(sell_price_paise)
    .bind(stock_qty)
    .bind(unit.as_str())
    .bind(low_stock_threshold)
    .bind(description)
    .bind(location)
    .bind(image)
    .bind(gst_rate_bp)
    .fetch_one(pool)
    .await?;

    Ok(item)
}

/// Edits an item's name/prices/unit/threshold/description/photo. Stock
/// quantity is deliberately not editable here — it only ever changes
/// through `record_purchase`/`record_sale`, so the transaction log always
/// explains how the current stock number was reached.
pub async fn edit_item(
    pool: &SqlitePool,
    id: i64,
    name: &str,
    buy_price_paise: i64,
    sell_price_paise: i64,
    unit: Unit,
    low_stock_threshold: f64,
    description: &str,
    location: &str,
    image: Option<&[u8]>,
    gst_rate_bp: Option<i64>,
) -> Result<Item, RepoError> {
    if name_taken(pool, name, Some(id)).await? {
        return Err(RepoError::DuplicateItemName { name: name.to_string() });
    }

    let item = sqlx::query_as::<_, Item>(concat!(
        "UPDATE items \
         SET name = ?, buy_price_paise = ?, sell_price_paise = ?, unit = ?, low_stock_threshold = ?, \
             description = ?, location = ?, image = ?, gst_rate_bp = ? \
         WHERE id = ? AND deleted = 0 \
         RETURNING ", item_columns!()
    ))
    .bind(name)
    .bind(buy_price_paise)
    .bind(sell_price_paise)
    .bind(unit.as_str())
    .bind(low_stock_threshold)
    .bind(description)
    .bind(location)
    .bind(image)
    .bind(gst_rate_bp)
    .bind(id)
    .fetch_optional(pool)
    .await?;

    item.ok_or(RepoError::ItemNotFound)
}

/// Soft delete — see the `deleted` column comment in the migration for why.
pub async fn delete_item(pool: &SqlitePool, id: i64) -> Result<(), RepoError> {
    let affected = sqlx::query("UPDATE items SET deleted = 1 WHERE id = ? AND deleted = 0")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(RepoError::ItemNotFound);
    }
    Ok(())
}

pub async fn list_items(pool: &SqlitePool) -> Result<Vec<Item>, RepoError> {
    let items = sqlx::query_as::<_, Item>(concat!(
        "SELECT ", item_columns!(), " FROM items WHERE deleted = 0 ORDER BY name"
    ))
    .fetch_all(pool)
    .await?;
    Ok(items)
}

/// Fetches just the photo bytes for one item — kept separate from the
/// lightweight `Item` shape so lists and pickers never pull image blobs
/// into memory just to show a name and a price.
pub async fn get_item_image(pool: &SqlitePool, id: i64) -> Result<Option<Vec<u8>>, RepoError> {
    let image: Option<Vec<u8>> = sqlx::query_scalar("SELECT image FROM items WHERE id = ? AND deleted = 0")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .flatten();
    Ok(image)
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

    #[tokio::test]
    async fn a_location_survives_the_round_trip_and_may_be_blank() {
        let pool = test_pool().await;

        let placed = add_item(&pool, "Hex Bolt", 100, 200, 5.0, Unit::Piece, 2.0, "", "Rack 4, Shelf B", None, None)
            .await
            .unwrap();
        let unplaced = add_item(&pool, "Hex Nut", 50, 90, 5.0, Unit::Piece, 2.0, "", "", None, None).await.unwrap();

        assert_eq!(placed.location, "Rack 4, Shelf B");
        assert_eq!(unplaced.location, "");

        // And it comes back on the list the whole app reads from.
        let listed = list_items(&pool).await.unwrap();
        let found = listed.iter().find(|i| i.id == placed.id).unwrap();
        assert_eq!(found.location, "Rack 4, Shelf B");
    }

    #[tokio::test]
    async fn editing_an_item_can_move_it_to_another_rack() {
        let pool = test_pool().await;
        let item = add_item(&pool, "Hex Bolt", 100, 200, 5.0, Unit::Piece, 2.0, "", "Rack 4", None, None)
            .await
            .unwrap();

        let moved = edit_item(&pool, item.id, "Hex Bolt", 100, 200, Unit::Piece, 2.0, "", "Godown", None, None)
            .await
            .unwrap();

        assert_eq!(moved.location, "Godown");
        // Stock is untouched by an edit — it only moves through purchases
        // and sales.
        assert_eq!(moved.stock_qty, 5.0);
    }
}
