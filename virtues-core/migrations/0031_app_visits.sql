-- 0031_app_visits
--
-- The visits log: one row each time the owner opens a chat, page or project
-- through a gesture of their own — a tab activated by a click, a row chosen
-- from the panel or ⌘K — and not when a tab is restored on reload or the
-- assistant opens something on their behalf (the client makes that
-- distinction; see `visits.svelte.ts`).
--
-- An append-only log, not a counter on the object tables, because the thing
-- worth knowing is frecency — how often, weighted by how recently — and a
-- counter cannot decay. Firefox's address bar, Slack's switcher and Spotlight
-- all rank this way. `api::visits::frecency` computes the score on read from
-- the last 90 days; the sweeper prunes older rows.
--
-- Nothing on a screen shows the number. It orders ⌘K, and later the Home
-- page's Recent and the archive nudge. "Viewed 14 times" is the register this
-- app refuses.

CREATE TABLE app_visits (
    id          BIGSERIAL PRIMARY KEY,
    kind        TEXT        NOT NULL,   -- chat | page | project
    record_id   TEXT        NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The frecency read groups by record over a window; the prune reads by time.
CREATE INDEX idx_app_visits_record      ON app_visits (kind, record_id, occurred_at DESC);
CREATE INDEX idx_app_visits_occurred_at ON app_visits (occurred_at);
