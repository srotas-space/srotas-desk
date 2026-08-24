-- Items: one row per stock-keeping item at the counter.
-- Money is stored as integer paise (rupees * 100) so totals never drift
-- from floating-point rounding — critical for a billing app.
CREATE TABLE items (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    name                TEXT    NOT NULL,
    buy_price_paise     INTEGER NOT NULL CHECK (buy_price_paise >= 0),
    sell_price_paise    INTEGER NOT NULL CHECK (sell_price_paise >= 0),
    stock_qty           REAL    NOT NULL DEFAULT 0 CHECK (stock_qty >= 0),
    unit                TEXT    NOT NULL DEFAULT 'piece' CHECK (unit IN ('piece', 'kg', 'metre')),
    low_stock_threshold REAL    NOT NULL DEFAULT 5,
    -- Soft delete: transactions keep referencing this row by item_id, so a
    -- deleted item can't be removed outright without corrupting past
    -- purchase/sale history and profit reports. Deleted items are just
    -- hidden from the active item list.
    deleted             INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1))
);

-- One row per buy/sell transaction. price_paise is the price *at the time
-- of this transaction*, independent of the item's current buy/sell price,
-- so past bills and profit numbers don't change if prices are edited later.
CREATE TABLE transactions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id     INTEGER NOT NULL REFERENCES items(id),
    type        TEXT    NOT NULL CHECK (type IN ('buy', 'sell')),
    qty         REAL    NOT NULL CHECK (qty > 0),
    price_paise INTEGER NOT NULL CHECK (price_paise >= 0),
    timestamp   TEXT    NOT NULL -- ISO 8601, e.g. 2026-08-24T10:15:00Z
);

CREATE INDEX idx_transactions_item_id ON transactions(item_id);
CREATE INDEX idx_transactions_timestamp ON transactions(timestamp);
CREATE INDEX idx_items_name ON items(name);
