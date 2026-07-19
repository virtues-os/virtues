-- Pre-order shipping details.
--
-- The deposit Checkout session now collects a US shipping address
-- (shipping_address_collection, restricted to allowed_countries). Persist it
-- here so fulfillment has somewhere to ship the unit — Stripe is the source of
-- truth, but we keep a local copy so the fulfillment flow never has to round-
-- trip to Stripe per order.
--
-- Captured by the webhook on `checkout.session.completed` from the session's
-- shipping_details (name + address). Backfilled NULL for any deposit taken
-- before this column existed.

ALTER TABLE preorders
    ADD COLUMN ship_name    text,            -- recipient name from Checkout
    ADD COLUMN ship_address jsonb,           -- full address object as Stripe returns it
    ADD COLUMN ship_country text;            -- ISO-3166 alpha-2, denormalized for filtering

CREATE INDEX preorders_ship_country_idx ON preorders (ship_country);
