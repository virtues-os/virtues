-- Finish the voucher → api_key rename (cosmetic; no behavior change).
--
-- 0008 renamed customers.billing_token_hash → api_key_hash and made the
-- billing_token the api_key in spirit, but three legacy names survived:
--   * device_link.billing_token — the one-time transport column that carries
--     the minted api_key to the polling box. The handler already binds it as
--     `api_key`; this aligns the column name.
--   * customers_billing_token_idx — the index over (now) api_key_hash kept its
--     old name through the 0008 column rename.
--   * customers.last_voucher_issued_at — the dead anti-stacking gate 0008
--     parked for one release. Nothing references it now.

ALTER TABLE device_link RENAME COLUMN billing_token TO api_key;

ALTER INDEX customers_billing_token_idx RENAME TO customers_api_key_hash_idx;

ALTER TABLE customers DROP COLUMN last_voucher_issued_at;
