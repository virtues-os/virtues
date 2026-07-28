-- Where you've been in the app.
--
-- The sidebar's "Recents" needs a real answer to "what was I just looking at",
-- across chats, pages, notebooks, records and assets alike. Each of those has
-- its own table with its own idea of "recent" (`updated_at`, `last_message_at`,
-- …), and none of them knows you *opened* something without changing it —
-- reading a PDF leaves no trace anywhere. So visits get their own log.
--
-- Sibling to `app_pins`: pins are the routes you chose to keep, this is the
-- routes you've been. Both key off the same `url` convention (`/page/page_x`,
-- `/chat/chat_x`, `/record/{ontology}/{id}`), so a row here can be pinned and a
-- pin can be found here without translation.
--
-- Append-only, rolled up on read. Storing one row per visit and collapsing to
-- latest-per-url in the query keeps the *sequence* — which is the thing that
-- makes it a history rather than a sorted list — while stopping one page
-- visited fifty times from filling the sidebar. `prune_app_history()` bounds
-- the growth that buys.

CREATE TABLE app_history (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    -- The route visited. Not a foreign key: history outlives its targets, and a
    -- deleted page's row should decay out of the window on its own rather than
    -- vanish the instant the page does (which would make "recent" lie about
    -- where you just were).
    url         TEXT NOT NULL,

    -- Title as it was at visit time. A fallback only — the client resolves the
    -- current title from the url where it can, so a renamed page doesn't show
    -- its old name forever. This is what's left when the target is gone.
    label       TEXT,
    icon        TEXT,

    -- Coarse bucket for the filter menu: 'chat' | 'page' | 'notebook' |
    -- 'record' | 'asset' | 'view'. Deliberately not a CHECK constraint — new
    -- surfaces shouldn't need a migration to appear in your history.
    kind        TEXT,

    visited_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The read path is always "most recent first, collapsed per url", optionally
-- narrowed by kind. This index serves the ordering and the DISTINCT ON.
CREATE INDEX idx_app_history_recent ON app_history(visited_at DESC);
CREATE INDEX idx_app_history_url_recent ON app_history(url, visited_at DESC);
CREATE INDEX idx_app_history_kind_recent ON app_history(kind, visited_at DESC);

-- Bound the log. Both limits apply: a 90-day window so history stays a record
-- of the recent past rather than of everything, and a row cap so a burst of
-- navigation can't blow past the window's intent. Called opportunistically on
-- write, not scheduled — this is cheap and there's no value in a timer.
CREATE OR REPLACE FUNCTION prune_app_history() RETURNS void AS $$
BEGIN
    DELETE FROM app_history WHERE visited_at < now() - INTERVAL '90 days';

    DELETE FROM app_history
    WHERE id IN (
        SELECT id FROM app_history
        ORDER BY visited_at DESC
        OFFSET 20000
    );
END;
$$ LANGUAGE plpgsql;
