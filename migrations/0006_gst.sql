-- GST (CGST + SGST, intra-state) support for billing.
--
-- Rates are stored as basis points where 1 unit = 0.01%, e.g. 1800 means
-- 18.00% — the same "hundredths" convention already used for paise, so the
-- existing money-formatting helpers can be reused for rate input/display.
--
-- `items.gst_rate_bp` is nullable: NULL means "use the shop's default rate"
-- (`shop_profile.gst_rate_bp`), a per-item value overrides it. Whatever rate
-- actually applied is snapshotted onto `bill_items.gst_rate_bp` at billing
-- time, same reasoning as `bill_items.price_paise` — a past bill must keep
-- reading correctly even if the item's rate or the shop default changes later.
ALTER TABLE shop_profile ADD COLUMN gst_rate_bp INTEGER NOT NULL DEFAULT 0 CHECK (gst_rate_bp >= 0);
ALTER TABLE shop_profile ADD COLUMN gstin TEXT;

ALTER TABLE items ADD COLUMN gst_rate_bp INTEGER CHECK (gst_rate_bp IS NULL OR gst_rate_bp >= 0);

ALTER TABLE bills ADD COLUMN cgst_paise INTEGER NOT NULL DEFAULT 0 CHECK (cgst_paise >= 0);
ALTER TABLE bills ADD COLUMN sgst_paise INTEGER NOT NULL DEFAULT 0 CHECK (sgst_paise >= 0);

ALTER TABLE bill_items ADD COLUMN gst_rate_bp INTEGER NOT NULL DEFAULT 0 CHECK (gst_rate_bp >= 0);
ALTER TABLE bill_items ADD COLUMN cgst_paise INTEGER NOT NULL DEFAULT 0 CHECK (cgst_paise >= 0);
ALTER TABLE bill_items ADD COLUMN sgst_paise INTEGER NOT NULL DEFAULT 0 CHECK (sgst_paise >= 0);
