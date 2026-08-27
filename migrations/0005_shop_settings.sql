-- Lets the shopkeeper replace the generic Srotas logo shown in the header
-- with their own shop's logo.
ALTER TABLE shop_profile ADD COLUMN logo BLOB;
