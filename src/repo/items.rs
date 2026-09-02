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

/// Escapes LIKE's own wildcards so a shopkeeper searching for an item
/// literally named "50%" gets that item rather than a pattern match.
fn like_pattern(query: &str) -> String {
    let escaped = query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

/// One page of the catalogue, filtered in SQL rather than in memory.
///
/// Nothing in this app ever loads the whole `items` table any more: a shop
/// with a hundred thousand SKUs would otherwise pay for all of them on
/// every screen, and the counter only ever looks at a screenful. Callers
/// ask for exactly the rows they are about to draw.
///
/// `unit` and `low_stock_only` mirror the Inventory screen's filters; an
/// empty `query` means "no name filter" rather than "match nothing".
pub async fn list_items_page(
    pool: &SqlitePool,
    query: &str,
    unit: Option<Unit>,
    low_stock_only: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<Item>, RepoError> {
    let query = query.trim();
    let pattern = like_pattern(query);
    let has_query = !query.is_empty();

    let items = sqlx::query_as::<_, Item>(concat!(
        "SELECT ", item_columns!(), " FROM items \
         WHERE deleted = 0 \
           AND (? = 0 OR name LIKE ? ESCAPE '\\') \
           AND (? IS NULL OR unit = ?) \
           AND (? = 0 OR stock_qty < low_stock_threshold) \
         ORDER BY name \
         LIMIT ? OFFSET ?"
    ))
    .bind(has_query as i64)
    .bind(&pattern)
    .bind(unit.map(|u| u.as_str()))
    .bind(unit.map(|u| u.as_str()))
    .bind(low_stock_only as i64)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(items)
}

/// How many items match the same filters — the total behind a page
/// indicator, without holding the rows it counts.
pub async fn count_items(
    pool: &SqlitePool,
    query: &str,
    unit: Option<Unit>,
    low_stock_only: bool,
) -> Result<i64, RepoError> {
    let query = query.trim();
    let pattern = like_pattern(query);
    let has_query = !query.is_empty();

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM items \
         WHERE deleted = 0 \
           AND (? = 0 OR name LIKE ? ESCAPE '\\') \
           AND (? IS NULL OR unit = ?) \
           AND (? = 0 OR stock_qty < low_stock_threshold)",
    )
    .bind(has_query as i64)
    .bind(&pattern)
    .bind(unit.map(|u| u.as_str()))
    .bind(unit.map(|u| u.as_str()))
    .bind(low_stock_only as i64)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// Candidates for an item picker: a short list, matched by name.
///
/// With no query this returns the first `limit` items alphabetically —
/// enough to browse from, not the whole catalogue. Every keystroke
/// re-queries, so the picker never holds more than a screenful.
pub async fn search_items(pool: &SqlitePool, query: &str, limit: i64) -> Result<Vec<Item>, RepoError> {
    list_items_page(pool, query, None, false, limit, 0).await
}

/// One item by id — for the screens that need the full row for whatever
/// is currently selected, now that no screen holds the catalogue.
pub async fn get_item(pool: &SqlitePool, id: i64) -> Result<Option<Item>, RepoError> {
    let item = sqlx::query_as::<_, Item>(concat!(
        "SELECT ", item_columns!(), " FROM items WHERE id = ? AND deleted = 0"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(item)
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

    /// Seeds a catalogue big enough to page through.
    async fn seed(pool: &SqlitePool, names: &[(&str, Unit, f64, f64)]) {
        for (name, unit, stock, threshold) in names {
            add_item(pool, name, 100, 200, *stock, *unit, *threshold, "", "", None, None).await.unwrap();
        }
    }

    #[tokio::test]
    async fn a_page_holds_only_its_own_rows() {
        let pool = test_pool().await;
        let items: Vec<(String, Unit, f64, f64)> =
            (0..25).map(|i| (format!("Item {i:02}"), Unit::Piece, 10.0, 5.0)).collect();
        for (n, u, s, t) in &items {
            add_item(&pool, n, 100, 200, *s, *u, *t, "", "", None, None).await.unwrap();
        }

        let first = list_items_page(&pool, "", None, false, 10, 0).await.unwrap();
        let third = list_items_page(&pool, "", None, false, 10, 20).await.unwrap();

        assert_eq!(first.len(), 10, "a page never returns more than its limit");
        assert_eq!(first[0].name, "Item 00");
        assert_eq!(third.len(), 5, "the last page returns only what is left");
        assert_eq!(count_items(&pool, "", None, false).await.unwrap(), 25);
    }

    #[tokio::test]
    async fn search_matches_on_name_and_counts_agree_with_rows() {
        let pool = test_pool().await;
        seed(
            &pool,
            &[
                ("PVC Pipe 1 inch", Unit::Metre, 10.0, 5.0),
                ("PVC Elbow", Unit::Piece, 10.0, 5.0),
                ("Copper Wire", Unit::Metre, 10.0, 5.0),
            ],
        )
        .await;

        let hits = list_items_page(&pool, "pvc", None, false, 20, 0).await.unwrap();
        assert_eq!(hits.len(), 2, "search is case-insensitive");
        assert_eq!(count_items(&pool, "pvc", None, false).await.unwrap(), 2);

        // An empty query is "no filter", not "match nothing".
        assert_eq!(count_items(&pool, "", None, false).await.unwrap(), 3);
        assert_eq!(count_items(&pool, "   ", None, false).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn a_percent_sign_in_a_search_is_literal_not_a_wildcard() {
        let pool = test_pool().await;
        seed(&pool, &[("50% Grey Cement", Unit::Kg, 10.0, 5.0), ("Copper Wire", Unit::Metre, 10.0, 5.0)]).await;

        // Unescaped, "%" would match every row.
        let hits = list_items_page(&pool, "%", None, false, 20, 0).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "50% Grey Cement");
    }

    #[tokio::test]
    async fn the_unit_and_low_stock_filters_narrow_the_result() {
        let pool = test_pool().await;
        seed(
            &pool,
            &[
                ("Pipe", Unit::Metre, 1.0, 5.0),   // metre, low
                ("Wire", Unit::Metre, 50.0, 5.0),  // metre, healthy
                ("Bolt", Unit::Piece, 1.0, 5.0),   // piece, low
            ],
        )
        .await;

        assert_eq!(count_items(&pool, "", Some(Unit::Metre), false).await.unwrap(), 2);
        assert_eq!(count_items(&pool, "", None, true).await.unwrap(), 2, "two are below threshold");
        assert_eq!(
            count_items(&pool, "", Some(Unit::Metre), true).await.unwrap(),
            1,
            "filters combine rather than replace each other"
        );
    }

    #[tokio::test]
    async fn a_picker_never_returns_more_than_its_limit() {
        let pool = test_pool().await;
        for i in 0..50 {
            add_item(&pool, &format!("Item {i:02}"), 100, 200, 1.0, Unit::Piece, 5.0, "", "", None, None)
                .await
                .unwrap();
        }

        assert_eq!(search_items(&pool, "", 20).await.unwrap().len(), 20);
        assert_eq!(search_items(&pool, "Item 1", 20).await.unwrap().len(), 10, "Item 10..19");
    }

    #[tokio::test]
    async fn deleted_items_disappear_from_every_listing() {
        let pool = test_pool().await;
        let item = add_item(&pool, "Ghost", 100, 200, 1.0, Unit::Piece, 5.0, "", "", None, None).await.unwrap();
        delete_item(&pool, item.id).await.unwrap();

        assert_eq!(count_items(&pool, "", None, false).await.unwrap(), 0);
        assert!(list_items_page(&pool, "", None, false, 20, 0).await.unwrap().is_empty());
        assert!(search_items(&pool, "Ghost", 20).await.unwrap().is_empty());
        assert!(get_item(&pool, item.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_item_returns_one_row_by_id() {
        let pool = test_pool().await;
        let added = add_item(&pool, "Brass Tap", 6500, 8900, 3.0, Unit::Piece, 8.0, "", "Rack 2", None, None)
            .await
            .unwrap();

        let found = get_item(&pool, added.id).await.unwrap().expect("just added");
        assert_eq!(found.name, "Brass Tap");
        assert_eq!(found.location, "Rack 2");
        assert!(get_item(&pool, 999_999).await.unwrap().is_none());
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

        // And it comes back on the page the Inventory screen reads.
        let listed = list_items_page(&pool, "", None, false, 50, 0).await.unwrap();
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
