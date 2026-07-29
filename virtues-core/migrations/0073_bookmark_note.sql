-- ---------------------------------------------------------------------------
-- Bookmarks: user-authored marginalia (docs/bookmarks-plan.md).
--
-- `note` holds whatever the user attaches at capture or review time — a
-- reason ("for the kitchen reno"), a todo ("check the pricing section"), a
-- pointer ("the chart at 12:30"). It is USER-AUTHORED ONLY: no transform,
-- enrichment pass, or model ever writes it. Machine-derived text (extraction
-- records, suggested tags) lives in `metadata` / derived tables, never here —
-- the retrieval boost this column will carry is a boost on the user's own
-- words, and blending generated text in would poison that signal.
--
-- Named `note`, not `why`: "why did you save this?" is the product prompt,
-- but the column stores general marginalia, and a name that presumes the
-- answer's grammar would be wrong the first time someone writes a todo.
-- ---------------------------------------------------------------------------

ALTER TABLE data_content_bookmark ADD COLUMN note TEXT;
