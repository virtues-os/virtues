-- Collapse to the linked prepaid model (v1).
--
-- The box's credential is now a single rotatable device `api_key` (replaces
-- the billing_token). atlas stores its hash to authenticate the box's billing
-- ops, and an opaque `account_id` per customer that it shares with virtues-api
-- to register devices + credit the wallet. Vouchers + anti-stacking are gone.

-- The billing_token becomes the api_key — same shape (SHA-256 hash), new name.
ALTER TABLE customers RENAME COLUMN billing_token_hash TO api_key_hash;

-- Opaque per-customer account id. Stable across re-links: rotating the api_key
-- re-points the device to the SAME account, so the wallet is preserved. Shared
-- with virtues-api (never a Stripe id / email).
ALTER TABLE customers ADD COLUMN account_id text;
-- Core SQL only (no pgcrypto / version dependency); matches the Rust
-- `acct_<32hex>` shape minted by new_account_id().
UPDATE customers
   SET account_id = 'acct_' || md5(random()::text || clock_timestamp()::text || stripe_customer_id)
 WHERE account_id IS NULL;
ALTER TABLE customers ALTER COLUMN account_id SET NOT NULL;
CREATE UNIQUE INDEX customers_account_id_idx ON customers (account_id);

-- `last_voucher_issued_at` (anti-stacking gate) is now dead. Kept nullable for
-- one release, dropped in a follow-up once nothing references it.
