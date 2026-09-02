-- Where an item physically lives in the shop. A counter hand who knows the
-- catalogue still has to walk to a rack to fetch the thing, and on a
-- catalogue of any size "which shelf?" is the question that actually costs
-- time. Free text on purpose — every shop numbers its racks differently,
-- and forcing a scheme on them would just get ignored.
ALTER TABLE items ADD COLUMN location TEXT NOT NULL DEFAULT '';

-- Who a bill was made out to. Optional: most counter sales are to whoever
-- is standing there, and stopping to type a name for a fifty-rupee packet
-- of screws would be worse than useless. It matters for the ones where a
-- customer wants their name on the invoice.
ALTER TABLE bills ADD COLUMN customer_name TEXT NOT NULL DEFAULT '';
