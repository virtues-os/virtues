-- Day-level data quality (W6H journalist assessment)
-- Stores JSON: {"coverage":{"who":3,...},"overall":3,"note":"..."}
-- Generated nightly alongside autobiography by the day summary LLM.
ALTER TABLE wiki_days ADD COLUMN data_quality TEXT;

-- Pages can be linked to a specific day (YYYY-MM-DD).
-- A page with a date is a reflection/journal entry for that day.
-- A page without a date is a regular page.
-- No unique constraint — a day can have many reflections.
ALTER TABLE app_pages ADD COLUMN date TEXT;
