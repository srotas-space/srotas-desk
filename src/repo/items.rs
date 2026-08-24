use sqlx::SqlitePool;

use super::RepoError;
use crate::models::{Item, Unit};

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
    image: Option<&[u8]>,
) -> Result<Item, RepoError> {
    if name_taken(pool, name, None).await? {
        return Err(RepoError::DuplicateItemName { name: name.to_string() });
    }

    let item = sqlx::query_as::<_, Item>(
        "INSERT INTO items (name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold, description, image) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         RETURNING id, name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold, deleted, description, (image IS NOT NULL) AS has_image",
    )
    .bind(name)
    .bind(buy_price_paise)
    .bind(sell_price_paise)
    .bind(stock_qty)
    .bind(unit.as_str())
    .bind(low_stock_threshold)
    .bind(description)
    .bind(image)
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
    image: Option<&[u8]>,
) -> Result<Item, RepoError> {
    if name_taken(pool, name, Some(id)).await? {
        return Err(RepoError::DuplicateItemName { name: name.to_string() });
    }

    let item = sqlx::query_as::<_, Item>(
        "UPDATE items \
         SET name = ?, buy_price_paise = ?, sell_price_paise = ?, unit = ?, low_stock_threshold = ?, \
             description = ?, image = ? \
         WHERE id = ? AND deleted = 0 \
         RETURNING id, name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold, deleted, description, (image IS NOT NULL) AS has_image",
    )
    .bind(name)
    .bind(buy_price_paise)
    .bind(sell_price_paise)
    .bind(unit.as_str())
    .bind(low_stock_threshold)
    .bind(description)
    .bind(image)
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
    let items = sqlx::query_as::<_, Item>(
        "SELECT id, name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold, deleted, \
                description, (image IS NOT NULL) AS has_image \
         FROM items WHERE deleted = 0 ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    Ok(items)
}

pub async fn search_items(pool: &SqlitePool, query: &str) -> Result<Vec<Item>, RepoError> {
    // Escape LIKE's own wildcards so a shopkeeper searching for e.g. a
    // product literally named "50%" doesn't get a pattern match instead.
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");

    let items = sqlx::query_as::<_, Item>(
        "SELECT id, name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold, deleted, \
                description, (image IS NOT NULL) AS has_image \
         FROM items WHERE deleted = 0 AND name LIKE ? ESCAPE '\\' ORDER BY name",
    )
    .bind(pattern)
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
