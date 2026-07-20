-- Document annotations (researcher-plan D2): highlights + margin notes on
-- drive files. Global to the FILE (visible from every notebook that holds it),
-- never trapped in one notebook — the reading surface is the file's, the
-- notebook is just a lens.
--
-- Anchoring is quote-based, not offset-based (researcher-plan decision 3): a
-- Rust-extractor char offset does not reliably index pdf.js's text layer.
-- quote_text + prefix/suffix context (W3C TextQuoteSelector style) re-find the
-- passage; `rects` are normalized page-space quads captured in-viewer at
-- creation time (render-scale independent) for drawing the highlight overlay.
CREATE TABLE IF NOT EXISTS app_annotations (
    id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL REFERENCES app_drive_files(id) ON DELETE CASCADE,
    -- 1-based page for paged formats (PDF); NULL for unpaged (text/markdown).
    page_num INT,
    -- The highlighted text, plus a little surrounding context to disambiguate
    -- when the same phrase appears more than once on a page.
    quote_text TEXT NOT NULL,
    quote_prefix TEXT NOT NULL DEFAULT '',
    quote_suffix TEXT NOT NULL DEFAULT '',
    -- Normalized page-space rectangles [{x,y,w,h} in 0..1], JSON array. Drawn
    -- as the highlight overlay at any zoom.
    rects JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Highlight color key (yellow|green|blue|pink…); the UI maps to CSS.
    color TEXT NOT NULL DEFAULT 'yellow',
    -- Optional margin note (markdown). Empty = a bare highlight.
    note_md TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_app_annotations_file
    ON app_annotations (file_id, page_num);
