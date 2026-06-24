-- Margin / usage reporting over the virtues-api ledger.
--
-- The ledger records every money movement; `charge` rows carry both `micros`
-- (the negative *billed* amount, post-20%-markup, debited from the wallet) and
-- `real_micros` (the *real* upstream cost we paid). So:
--     markup margin = billed − real_cost  (the 20% we keep on usage)
-- This is the *usage* margin only. Full business margin also includes Stripe
-- subscription revenue (lives in the atlas DB), minus this real cost.
--
-- Run against the virtues-api DB. On the prod box (no SSH — SSM only):
--   PURL=$(docker inspect virtues-api --format '{{range .Config.Env}}{{println .}}{{end}}' \
--          | grep '^VIRTUES_API_DATABASE_URL=' | cut -d= -f2-)
--   docker run --rm --network host -v "$PWD":/q postgres:16-alpine \
--     psql "$PURL" -P pager=off -f /q/margin.sql
-- (or paste a single query via `psql "$PURL" -c "…"`).

\echo '== All-time =='
SELECT
  count(*) FILTER (WHERE kind = 'charge')                              AS calls,
  round(-SUM(micros)      FILTER (WHERE kind = 'charge') / 1e6, 4)     AS billed_usd,      -- charged to wallets
  round( SUM(real_micros) FILTER (WHERE kind = 'charge') / 1e6, 4)     AS real_cost_usd,   -- paid upstream
  round((-SUM(micros) - SUM(real_micros))
                          FILTER (WHERE kind = 'charge') / 1e6, 4)     AS markup_margin_usd,-- the 20% we keep
  round( SUM(micros)      FILTER (WHERE kind = 'grant') / 1e6, 2)      AS grants_usd,      -- monthly renewals credited
  round( SUM(micros)      FILTER (WHERE kind = 'topup') / 1e6, 2)      AS topups_usd       -- card top-ups credited
FROM ledger;

\echo '== Per calendar month (usage) =='
SELECT
  date_trunc('month', ts)::date                    AS month,
  count(*)                                         AS calls,
  round(-SUM(micros)      / 1e6, 4)                AS billed_usd,
  round( SUM(real_micros) / 1e6, 4)                AS real_cost_usd,
  round((-SUM(micros) - SUM(real_micros)) / 1e6, 4) AS markup_margin_usd
FROM ledger
WHERE kind = 'charge'
GROUP BY 1
ORDER BY 1;

\echo '== Per account (this calendar month, top spenders) =='
SELECT
  account_id,
  count(*)                                         AS calls,
  round(-SUM(micros)      / 1e6, 4)                AS billed_usd,
  round( SUM(real_micros) / 1e6, 4)                AS real_cost_usd,
  round((-SUM(micros) - SUM(real_micros)) / 1e6, 4) AS markup_margin_usd
FROM ledger
WHERE kind = 'charge'
  AND ts >= date_trunc('month', now())
GROUP BY account_id
ORDER BY billed_usd DESC
LIMIT 50;
