-- A bill is a multi-item invoice: pick several items, apply one discount,
-- get one total — distinct from the single-item "Sell Stock" quick action
-- (which still writes directly to `transactions`). Bills keep their own
-- history and never touch `transactions`, by design: they're a separate
-- workflow, not a replacement for the quick sale.
CREATE TABLE bills (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    subtotal_paise  INTEGER NOT NULL CHECK (subtotal_paise >= 0),
    discount_paise  INTEGER NOT NULL DEFAULT 0 CHECK (discount_paise >= 0),
    total_paise     INTEGER NOT NULL CHECK (total_paise >= 0),
    timestamp       TEXT    NOT NULL,
    -- Soft delete, same reasoning as items: hides it from history without
    -- rewriting what actually happened. Deliberately does NOT restock —
    -- deleting a bill record isn't the same as reversing a sale.
    deleted         INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1))
);

CREATE TABLE bill_items (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    bill_id           INTEGER NOT NULL REFERENCES bills(id),
    item_id           INTEGER NOT NULL REFERENCES items(id),
    item_name         TEXT    NOT NULL, -- captured at billing time, so a bill still reads correctly if the item is later renamed/deleted
    qty               REAL    NOT NULL CHECK (qty > 0),
    price_paise       INTEGER NOT NULL CHECK (price_paise >= 0),
    line_total_paise  INTEGER NOT NULL CHECK (line_total_paise >= 0)
);
CREATE INDEX idx_bill_items_bill_id ON bill_items(bill_id);
CREATE INDEX idx_bills_timestamp ON bills(timestamp);
