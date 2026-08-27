-- Single-row table, same pattern as `shop_profile`. `device_id` is
-- generated the first time this app ever runs and never changes — it's
-- what a license key issued from the SaaS admin panel is bound to.
-- `key_text`/`activated_at` stay NULL until a valid key is entered on the
-- Activation screen; see `src/license.rs` for how a key is verified
-- (entirely offline, no server involved).
CREATE TABLE license (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    device_id    TEXT NOT NULL,
    key_text     TEXT,
    activated_at TEXT
);
