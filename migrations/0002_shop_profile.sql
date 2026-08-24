-- Single-row table holding the shop's identity, captured once at
-- first-run registration. The CHECK pins it to exactly one row — this app
-- is explicitly single-shop, single-counter (see project non-goals).
CREATE TABLE shop_profile (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    shop_name   TEXT    NOT NULL,
    owner_name  TEXT    NOT NULL DEFAULT '',
    phone       TEXT    NOT NULL DEFAULT '',
    address     TEXT    NOT NULL DEFAULT '',
    -- Optional PIN to keep a casual passerby from opening the till screen —
    -- this is a soft screen lock for a shared shop counter, not a security
    -- boundary, so it's stored as plain text on purpose. NULL = no PIN set.
    pin         TEXT,
    created_at  TEXT    NOT NULL
);
