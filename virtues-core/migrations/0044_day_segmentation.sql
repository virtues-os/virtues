-- ---------------------------------------------------------------------------
-- Split the day: segmenting it is not narrating it.
--
-- One LLM call produced the autobiography, the epigraph, the data-quality
-- assessment AND the events. Fusing them was the root of three problems:
--
--   * EVENTS COST OPUS PRICES. Cutting a day into spans is structured
--     extraction — grunt work — and it was being billed at the narrative rate.
--
--   * THE GATE WAS CIRCULAR. "Only narrate a day with enough good events" is
--     the right rule and it was unstatable, because the events did not exist
--     until the narration ran. Gating the day therefore also killed the events.
--
--   * THERE WAS NO HOURLY CRON. The plan called for one (events hourly, days
--     nightly). You cannot re-segment as data lands if re-segmenting means
--     re-writing the day's prose every time.
--
-- So: segmentation is hourly, cheap, factual, on the Lite slot. Narration is
-- nightly, expensive, interpretive, on the Chat slot — and it reads the EVENTS
-- rather than the raw sources, which the prompt always claimed it did ("not to
-- log what happened when — the event timeline already does that").
--
-- `sources_fingerprint` is what makes hourly safe. Re-segmenting DELETES and
-- re-creates every auto event, and event ids are content-addressed from their
-- boundaries — so a re-cut mints new ids, strands their index chunks, and
-- discards their scores. Doing that every hour on a day nothing has happened in
-- would be vandalism. The fingerprint is the day's source set; unchanged means
-- untouched, and no LLM call at all.
-- ---------------------------------------------------------------------------

ALTER TABLE wiki_days
    ADD COLUMN sources_fingerprint TEXT,
    ADD COLUMN segmented_at        TIMESTAMPTZ,
    ADD COLUMN narrated_at         TIMESTAMPTZ;

COMMENT ON COLUMN wiki_days.sources_fingerprint IS
    'What the day looked like when it was last segmented. Unchanged → nothing to '
    're-cut, so the hourly pass does no work and spends nothing.';

COMMENT ON COLUMN wiki_days.segmented_at IS
    'When the day was last cut into events (hourly, Lite slot).';

COMMENT ON COLUMN wiki_days.narrated_at IS
    'When the day was last written up (nightly, Chat slot, only if it earned it).';
