-- Getting-started sections the owner has dismissed from Home.
--
-- Home's first-run state is a set of sections that individually retire when
-- answered or dismissed (agents/plan/getting-started-plan.md). Dismissal is a
-- fact about the owner, not about one browser, so it lives on the profile
-- singleton rather than in localStorage — the page must shed identically on
-- every glass.
--
-- Values are section ids ('introductions', 'connect', 'interview', enrichment
-- row ids). An unknown id is harmless: the client only ever asks "does this
-- array contain my id", so stale ids from removed sections just sit inert.
ALTER TABLE app_user_profile
    ADD COLUMN getting_started_dismissed text[] NOT NULL DEFAULT '{}';
