-- Soft delete for transactions, same reasoning as items/bills: a voided
-- sale record is hidden from history without silently reversing its real
-- stock effect. Correcting a mistake (wrong qty/price) is a separate
-- operation — editing a sale reconciles stock by the delta, same as
-- editing a bill line; deleting one does not.
ALTER TABLE transactions ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1));
