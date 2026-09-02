-- The screen-lock PIN moves from plain text to an Argon2id hash.
--
-- `0002_shop_profile.sql` stored it in the clear on the grounds that it's a
-- soft counter lock rather than a security boundary. That reasoning doesn't
-- survive the backup feature: the whole database file gets copied onto
-- pendrives and synced folders, so a plaintext PIN travels far past the
-- counter it was typed at — and shopkeepers reuse PINs.
--
-- The old `pin` column is deliberately kept (not dropped) so an existing
-- install still has its PIN to re-hash. `repo::shop::upgrade_legacy_pin`
-- does that on the next launch and then blanks the column; hashing needs
-- Argon2, which SQL can't do, so it can't happen here.
ALTER TABLE shop_profile ADD COLUMN pin_hash TEXT;

-- Failed-attempt throttling for the login screen. Persisted rather than
-- held in memory so that quitting and relaunching the app doesn't wipe the
-- lockout — otherwise it would be trivially bypassable.
ALTER TABLE shop_profile ADD COLUMN pin_failed_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE shop_profile ADD COLUMN pin_locked_until TEXT;
