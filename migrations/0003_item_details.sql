ALTER TABLE items ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE items ADD COLUMN image BLOB;

-- Prevents two active items from sharing a name (case-insensitive). This is
-- the DB-level backstop behind the friendly duplicate-name check the app
-- does before every insert/update.
CREATE UNIQUE INDEX idx_items_name_unique ON items (name COLLATE NOCASE) WHERE deleted = 0;
