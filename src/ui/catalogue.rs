//! Filtering and paging over a catalogue held in memory.
//!
//! The app offers two ways of getting items onto a screen, chosen under
//! Settings → Performance:
//!
//! * **Queried on demand** (the default) — every screen asks the database
//!   for exactly the rows it is about to draw. Costs the same whether the
//!   shop stocks fifty items or a hundred thousand. See `repo::items`.
//! * **Held in memory** — the whole catalogue is loaded once and every
//!   screen slices it here, with no database round-trip per keystroke.
//!
//! The functions below are the second mode, and they exist as pure
//! functions over a slice precisely because that half is the one at risk
//! of quietly rotting: it isn't the default, so it isn't exercised unless
//! somebody turns it on. Being pure, it can be tested against the same
//! expectations as the SQL — see the tests at the bottom, which pin the
//! behaviours the two modes must agree on.
//!
//! **Every function here mirrors a query in `repo::items` or
//! `repo::reports`.** Change one and you must change its twin, or the two
//! modes will quietly disagree about what the shopkeeper is looking at.
use crate::models::{Item, Unit};

/// Does `item` pass the Inventory screen's filters?
///
/// Mirrors the `WHERE` clause of `repo::items::list_items_page`. SQLite's
/// `LIKE` is case-insensitive across ASCII, which is what the lowercase
/// comparison here reproduces; the two can differ on non-ASCII names,
/// where SQLite compares them case-sensitively and this does not.
pub fn matches(item: &Item, query: &str, unit: Option<Unit>, low_stock_only: bool) -> bool {
    let query = query.trim();
    if !query.is_empty() && !item.name.to_lowercase().contains(&query.to_lowercase()) {
        return false;
    }
    if let Some(unit) = unit {
        if item.unit != unit.as_str() {
            return false;
        }
    }
    if low_stock_only && !item.is_low_stock() {
        return false;
    }
    true
}

/// One page of the catalogue, plus how many items matched and which page
/// was actually used.
///
/// Mirrors `repo::items::list_items_page` + `count_items`, down to the
/// ordering (`ORDER BY name`, which SQLite does byte-wise for a column
/// with no collation) and the page clamping — deleting the last item on
/// the last page must not strand the list on a page that no longer
/// exists.
pub fn page(
    items: &[Item],
    query: &str,
    unit: Option<Unit>,
    low_stock_only: bool,
    page_size: usize,
    page: usize,
) -> (Vec<Item>, i64, usize) {
    let mut matched: Vec<&Item> = items.iter().filter(|i| matches(i, query, unit, low_stock_only)).collect();
    matched.sort_by(|a, b| a.name.cmp(&b.name));

    let total = matched.len();
    let page_count = total.div_ceil(page_size).max(1);
    let page = page.min(page_count - 1);

    let rows = matched.into_iter().skip(page * page_size).take(page_size).cloned().collect();
    (rows, total as i64, page)
}

/// Candidates for an item picker — mirrors `repo::items::search_items`.
///
/// In this mode the limit is a courtesy rather than a necessity: the rows
/// are already in memory, so a wider net costs nothing to fetch. It stays
/// because a picker listing thousands of options is unusable regardless
/// of where they came from.
pub fn search(items: &[Item], query: &str, limit: usize) -> Vec<Item> {
    let (rows, _, _) = page(items, query, None, false, limit, 0);
    rows
}

/// One page of the items below their own threshold.
///
/// Mirrors `repo::reports::low_stock_page`, including its ordering: by how
/// far below threshold each item is, so the most urgent restock is on page
/// one rather than wherever the alphabet puts it.
///
/// An item whose threshold is zero can never be low (stock is
/// non-negative by schema), so the divide-by-zero the SQL guards with
/// `NULLIF` cannot arise here either.
pub fn low_stock_page(items: &[Item], page_size: usize, page: usize) -> (Vec<Item>, i64, usize) {
    let mut matched: Vec<&Item> = items.iter().filter(|i| i.is_low_stock()).collect();
    matched.sort_by(|a, b| {
        let ratio = |i: &Item| i.stock_qty / i.low_stock_threshold;
        ratio(a).partial_cmp(&ratio(b)).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.name.cmp(&b.name))
    });

    let total = matched.len();
    let page_count = total.div_ceil(page_size).max(1);
    let page = page.min(page_count - 1);

    let rows = matched.into_iter().skip(page * page_size).take(page_size).cloned().collect();
    (rows, total as i64, page)
}

/// How many items are below their threshold — the Home screen's badge.
/// Mirrors `repo::reports::low_stock_count`.
///
/// No caller today — `low_stock_page` returns the same total alongside
/// its rows, so nothing needs to ask separately. It stays because the
/// parity contract at the top of this file is the point: the SQL side has
/// this query, so the in-memory side must have its twin, and the tests
/// below pin them to the same answer. Deleting it would leave that half
/// of the contract unchecked.
#[allow(dead_code)]
pub fn low_stock_count(items: &[Item]) -> i64 {
    items.iter().filter(|i| i.is_low_stock()).count() as i64
}

/// The two modes, run against the same data and required to agree.
///
/// These are the tests that actually protect the contract at the top of
/// this file: the pure functions above are checked against the SQL in
/// `repo`, not against hand-written expectations, so a change to either
/// side that breaks parity fails here rather than in a shop.
#[cfg(test)]
mod parity {
    use super::*;
    use crate::repo;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::SqlitePool;
    use std::str::FromStr;

    async fn seeded_pool() -> (SqlitePool, Vec<Item>) {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        // Deliberately awkward: mixed units, mixed case, names that sort
        // differently under byte order than under a naive comparison, and
        // a spread of low-stock ratios.
        let rows = [
            ("Brass Tap", "piece", 3.0, 8.0),
            ("anchor bolt", "piece", 50.0, 5.0),
            ("Cement Bag", "kg", 1.0, 4.0),
            ("Wall Putty", "kg", 0.0, 5.0),
            ("Copper Wire", "metre", 90.0, 25.0),
            ("PVC Pipe 1 inch", "metre", 2.0, 10.0),
            ("PVC Pipe 2 inch", "metre", 40.0, 10.0),
            ("Zinc Washer", "piece", 1.0, 20.0),
        ];
        for (name, unit, stock, threshold) in rows {
            let unit = Unit::parse(unit).unwrap();
            repo::add_item(&pool, name, 100, 200, stock, unit, threshold, "", "", None, None).await.unwrap();
        }

        // The resident copy the in-memory mode would hold.
        let resident = repo::list_items_page(&pool, "", None, false, i64::MAX, 0).await.unwrap();
        (pool, resident)
    }

    fn names(items: &[Item]) -> Vec<String> {
        items.iter().map(|i| i.name.clone()).collect()
    }

    #[tokio::test]
    async fn both_modes_page_the_catalogue_identically() {
        let (pool, resident) = seeded_pool().await;

        for page_size in [3_usize, 5] {
            for page_idx in 0..4_usize {
                let (mem_rows, mem_total, mem_page) = page(&resident, "", None, false, page_size, page_idx);
                let sql_total = repo::count_items(&pool, "", None, false).await.unwrap();
                let sql_rows = repo::list_items_page(
                    &pool,
                    "",
                    None,
                    false,
                    page_size as i64,
                    mem_page as i64 * page_size as i64,
                )
                .await
                .unwrap();

                assert_eq!(mem_total, sql_total, "totals differ at size {page_size} page {page_idx}");
                assert_eq!(names(&mem_rows), names(&sql_rows), "rows differ at size {page_size} page {page_idx}");
            }
        }
    }

    #[tokio::test]
    async fn both_modes_apply_the_same_filters() {
        let (pool, resident) = seeded_pool().await;

        let cases: [(&str, Option<Unit>, bool); 7] = [
            ("", None, false),
            ("pvc", None, false),
            ("PVC", None, false),   // LIKE is case-insensitive; so is `matches`
            ("bolt", None, false),  // matches a lowercase name
            ("", Some(Unit::Kg), false),
            ("", None, true),
            ("pipe", Some(Unit::Metre), true),
        ];

        for (query, unit, low_only) in cases {
            let (mem_rows, mem_total, _) = page(&resident, query, unit, low_only, 50, 0);
            let sql_total = repo::count_items(&pool, query, unit, low_only).await.unwrap();
            let sql_rows = repo::list_items_page(&pool, query, unit, low_only, 50, 0).await.unwrap();

            assert_eq!(mem_total, sql_total, "total differs for {query:?}/{unit:?}/{low_only}");
            assert_eq!(names(&mem_rows), names(&sql_rows), "rows differ for {query:?}/{unit:?}/{low_only}");
        }
    }

    #[tokio::test]
    async fn both_modes_order_low_stock_the_same_way() {
        let (pool, resident) = seeded_pool().await;

        let (mem_rows, mem_total, _) = low_stock_page(&resident, 50, 0);
        let sql_total = repo::low_stock_count(&pool).await.unwrap();
        let sql_rows = repo::low_stock_page(&pool, 50, 0).await.unwrap();

        assert_eq!(mem_total, sql_total);
        assert_eq!(low_stock_count(&resident), sql_total);
        // Ordering is the point: most urgent restock first, in both modes.
        assert_eq!(names(&mem_rows), names(&sql_rows));
    }

    #[tokio::test]
    async fn both_modes_offer_the_same_picker_candidates() {
        let (pool, resident) = seeded_pool().await;

        for query in ["", "pvc", "wire", "nothing-matches-this"] {
            let mem = search(&resident, query, 20);
            let sql = repo::search_items(&pool, query, 20).await.unwrap();
            assert_eq!(names(&mem), names(&sql), "picker differs for {query:?}");
        }
    }

    #[tokio::test]
    async fn a_limit_truncates_the_same_rows_in_both_modes() {
        let (pool, resident) = seeded_pool().await;

        let mem = search(&resident, "", 3);
        let sql = repo::search_items(&pool, "", 3).await.unwrap();
        assert_eq!(mem.len(), 3);
        assert_eq!(names(&mem), names(&sql));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64, name: &str, unit: &str, stock: f64, threshold: f64) -> Item {
        Item {
            id,
            name: name.to_string(),
            buy_price_paise: 100,
            sell_price_paise: 200,
            stock_qty: stock,
            unit: unit.to_string(),
            low_stock_threshold: threshold,
            description: String::new(),
            location: String::new(),
            has_image: false,
            gst_rate_bp: None,
        }
    }

    fn catalogue() -> Vec<Item> {
        vec![
            item(1, "Brass Tap", "piece", 3.0, 8.0),    // low, ratio 0.375
            item(2, "Cement Bag", "kg", 1.0, 4.0),      // low, ratio 0.25
            item(3, "Anchor Bolt", "piece", 50.0, 5.0), // healthy
            item(4, "Wall Putty", "kg", 0.0, 5.0),      // low, ratio 0.0
            item(5, "Copper Wire", "metre", 90.0, 25.0),// healthy
        ]
    }

    #[test]
    fn an_empty_query_matches_everything() {
        for i in catalogue() {
            assert!(matches(&i, "", None, false));
            assert!(matches(&i, "   ", None, false));
        }
    }

    #[test]
    fn the_name_filter_is_a_case_insensitive_substring() {
        let i = item(1, "Brass Tap", "piece", 1.0, 1.0);
        assert!(matches(&i, "brass", None, false));
        assert!(matches(&i, "BRASS", None, false));
        assert!(matches(&i, "ss ta", None, false));
        assert!(!matches(&i, "copper", None, false));
    }

    #[test]
    fn the_unit_and_low_stock_filters_compose() {
        let all = catalogue();
        let (rows, total, _) = page(&all, "", Some(Unit::Kg), false, 10, 0);
        assert_eq!(total, 2);
        assert!(rows.iter().all(|i| i.unit == "kg"));

        let (rows, total, _) = page(&all, "", Some(Unit::Kg), true, 10, 0);
        assert_eq!(total, 2, "both kg items happen to be low");
        assert!(rows.iter().all(|i| i.is_low_stock()));

        let (_, total, _) = page(&all, "", Some(Unit::Piece), true, 10, 0);
        assert_eq!(total, 1, "only Brass Tap is a low piece");
    }

    #[test]
    fn pages_are_ordered_by_name_and_split_by_size() {
        let all = catalogue();
        let (first, total, page_idx) = page(&all, "", None, false, 2, 0);
        assert_eq!(total, 5);
        assert_eq!(page_idx, 0);
        assert_eq!(first.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["Anchor Bolt", "Brass Tap"]);

        let (second, _, _) = page(&all, "", None, false, 2, 1);
        assert_eq!(second.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["Cement Bag", "Copper Wire"]);

        let (last, _, _) = page(&all, "", None, false, 2, 2);
        assert_eq!(last.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["Wall Putty"]);
    }

    #[test]
    fn a_page_past_the_end_is_clamped_to_the_last_one() {
        // The case that matters: deleting the last item on the last page
        // must not leave the list stranded on a page that is now empty.
        let all = catalogue();
        let (rows, _, page_idx) = page(&all, "", None, false, 2, 99);
        assert_eq!(page_idx, 2);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn an_empty_catalogue_still_reports_one_page() {
        let (rows, total, page_idx) = page(&[], "", None, false, 10, 3);
        assert!(rows.is_empty());
        assert_eq!((total, page_idx), (0, 0));
    }

    #[test]
    fn low_stock_is_ordered_by_how_far_below_threshold() {
        let all = catalogue();
        let (rows, total, _) = low_stock_page(&all, 10, 0);
        assert_eq!(total, 3);
        assert_eq!(
            rows.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            ["Wall Putty", "Cement Bag", "Brass Tap"],
            "most urgent first: ratios 0.0, 0.25, 0.375"
        );
        assert_eq!(low_stock_count(&all), 3);
    }

    #[test]
    fn low_stock_paginates_and_clamps() {
        let all = catalogue();
        let (rows, total, page_idx) = low_stock_page(&all, 2, 0);
        assert_eq!((rows.len(), total, page_idx), (2, 3, 0));

        let (rows, _, page_idx) = low_stock_page(&all, 2, 9);
        assert_eq!((rows.len(), page_idx), (1, 1));
    }

    #[test]
    fn search_returns_at_most_the_limit() {
        let all = catalogue();
        assert_eq!(search(&all, "", 2).len(), 2);
        assert_eq!(search(&all, "", 99).len(), 5);

        let hits = search(&all, "a", 10);
        assert!(hits.iter().all(|i| i.name.to_lowercase().contains('a')));
    }

    /// An item exactly *at* its threshold is not low — the schema's rule is
    /// `stock_qty < low_stock_threshold`, and both modes must agree on the
    /// boundary or the same item appears low in one and not the other.
    #[test]
    fn the_low_stock_boundary_is_strictly_below() {
        let at = item(1, "At", "piece", 5.0, 5.0);
        let below = item(2, "Below", "piece", 4.9, 5.0);
        assert!(!at.is_low_stock());
        assert!(below.is_low_stock());
        assert_eq!(low_stock_count(&[at, below]), 1);
    }
}
