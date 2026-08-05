-- Which purse paid for an AI call: the Virtues wallet, or the user's own key.
--
-- `cost_micros` is authoritative ONLY for wallet calls, where the gateway
-- returns `usage.cost`. No other upstream reports a price, so a BYO row lands
-- at 0 — and 0 there means "we do not know", not "free". Without this column
-- the two are indistinguishable, and the Usage tab would read a month of BYO
-- traffic as $0.00 spent: a true statement about the wallet that reads as a
-- broken page.
--
-- So: sum cost only over `route = 'wallet'`, and show tokens for the rest.
-- Every existing row predates BYO routing, so 'wallet' is the correct backfill.
--
-- Named `route` rather than `is_byo` because it is the axis the plan splits
-- slots along — per-slot routing (docs/byo-ai-plan.md phase 3) adds values here
-- rather than a second boolean.
ALTER TABLE app_ai_calls
    ADD COLUMN route TEXT NOT NULL DEFAULT 'wallet';

-- The Usage tab groups by this alongside the existing created_at ordering.
CREATE INDEX idx_app_ai_calls_route ON app_ai_calls (route, created_at);
