use chrono::{DateTime, Utc};

/// The unit an item is stocked/sold in. Kept as a typed enum here for
/// validation, but stored in the DB as plain TEXT (see `as_str`/`parse`) —
/// SQLite has no native enum type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Piece,
    Kg,
    Metre,
}

impl Unit {
    pub fn as_str(self) -> &'static str {
        match self {
            Unit::Piece => "piece",
            Unit::Kg => "kg",
            Unit::Metre => "metre",
        }
    }

    pub fn parse(s: &str) -> Option<Unit> {
        match s {
            "piece" => Some(Unit::Piece),
            "kg" => Some(Unit::Kg),
            "metre" => Some(Unit::Metre),
            _ => None,
        }
    }

    pub const ALL: [Unit; 3] = [Unit::Piece, Unit::Kg, Unit::Metre];
}

impl std::fmt::Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row from the `items` table. Deliberately excludes the `image` BLOB —
/// this is the lightweight shape used for lists and pickers; fetch the
/// photo separately (`repo::get_item_image`) only when a screen actually
/// needs to render it, so loading the catalog never means loading every
/// product photo into memory at once.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Item {
    pub id: i64,
    pub name: String,
    pub buy_price_paise: i64,
    pub sell_price_paise: i64,
    pub stock_qty: f64,
    pub unit: String,
    pub low_stock_threshold: f64,
    pub deleted: bool,
    pub description: String,
    pub has_image: bool,
    /// Basis points (1800 = 18.00%). `None` means "use the shop's default
    /// GST rate" rather than "0% GST" — see `ShopProfile::gst_rate_bp`.
    pub gst_rate_bp: Option<i64>,
}

impl Item {
    pub fn is_low_stock(&self) -> bool {
        self.stock_qty < self.low_stock_threshold
    }
}

/// One row from the `transactions` table. Field is named `kind` rather than
/// `type` because `type` is a Rust keyword — the SQL still uses `type` and
/// aliases it to `kind` on the way out.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Transaction {
    pub id: i64,
    pub item_id: i64,
    pub kind: String,
    pub qty: f64,
    pub price_paise: i64,
    pub timestamp: DateTime<Utc>,
}

/// The single row from `shop_profile` — this shop's identity, captured at
/// first-run registration.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ShopProfile {
    pub shop_name: String,
    pub owner_name: String,
    pub phone: String,
    pub address: String,
    pub pin: Option<String>,
    pub created_at: DateTime<Utc>,
    pub has_logo: bool,
    /// Default GST rate in basis points (1800 = 18.00%), used for any item
    /// that doesn't set its own override.
    pub gst_rate_bp: i64,
    pub gstin: Option<String>,
}

/// One row of the bills list — enough to render a history row without
/// pulling in every line item.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BillSummary {
    pub id: i64,
    pub item_count: i64,
    pub total_paise: i64,
    pub timestamp: DateTime<Utc>,
}

/// One line of a bill. `item_name` is captured at billing time (not joined
/// live from `items`), so a bill still reads correctly even if the item is
/// later renamed or deleted.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BillLine {
    pub id: i64,
    pub item_id: i64,
    pub item_name: String,
    pub qty: f64,
    pub price_paise: i64,
    pub line_total_paise: i64,
    /// The GST rate actually applied to this line, snapshotted at billing
    /// time (basis points, 1800 = 18.00%) — independent of the item's or
    /// shop's *current* rate, same reasoning as `price_paise`.
    pub gst_rate_bp: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
}

#[derive(Debug, Clone)]
pub struct BillDetail {
    pub id: i64,
    pub subtotal_paise: i64,
    pub discount_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub total_paise: i64,
    pub timestamp: DateTime<Utc>,
    pub lines: Vec<BillLine>,
}
