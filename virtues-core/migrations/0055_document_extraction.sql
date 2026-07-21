-- Document extraction (researcher-plan D1): universal native-text extraction
-- over drive files. The whole drive is corpus; the notebook Library is a lens.
--
-- extraction_status lifecycle:
--   pending    — queued for the document_extraction cron
--   extracting — claimed by a run (crash-recovered back to pending by age)
--   done       — chunks written
--   no_text    — born-digital extraction found no text layer (scanned; the
--                future OCR queue — see researcher-plan D5)
--   failed     — extractor error (UI offers re-extract)
--   skipped    — not a text-bearing type
ALTER TABLE app_drive_files
    ADD COLUMN IF NOT EXISTS extraction_status TEXT NOT NULL DEFAULT 'skipped',
    ADD COLUMN IF NOT EXISTS extracted_at TIMESTAMPTZ;

-- Backfill: queue every existing text-bearing file (universal extraction).
UPDATE app_drive_files
SET extraction_status = 'pending'
WHERE is_folder = FALSE
  AND deleted_at IS NULL
  AND (
    mime_type IN (
        'application/pdf',
        'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
        'text/plain', 'text/markdown', 'text/html'
    )
    OR filename ~* '\.(pdf|docx|txt|md|markdown|html?)$'
  );

-- One row per retrieval chunk. Anchors: page_num = page where the chunk
-- STARTS (NULL for unpaged formats), char range into the extractor's canonical
-- full text (bookkeeping only — viewer landing is quote-based), quote_head =
-- leading snippet for self-contained citation links (?page=N&q=...).
-- Chunk ids are deterministic (file_id + chunk_index) so re-extraction
-- upserts in place.
CREATE TABLE IF NOT EXISTS extracted_document_chunks (
    id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL REFERENCES app_drive_files(id) ON DELETE CASCADE,
    chunk_index INT NOT NULL,
    page_num INT,
    char_start BIGINT NOT NULL,
    char_end BIGINT NOT NULL,
    quote_head TEXT NOT NULL,
    text TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (file_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_extracted_document_chunks_file
    ON extracted_document_chunks (file_id);
