-- Give document chunks a real event time.
--
-- `created_at` here is when the PARSER ran, and it was wired as the
-- ontology's timestamp_column — so every date-scoped search over documents
-- filtered on when the box parsed the file, and a re-extraction moved a
-- document to "today". The best event time the schema holds is the owning
-- file's created_at: when the document entered the record. (The date inside
-- the document — a 2019 PDF's own 2019 — is not extracted anywhere yet;
-- when metadata extraction exists, it writes this column better. Until
-- then, upload time beats parse time.)
ALTER TABLE extracted_document_chunks
    ADD COLUMN occurred_at timestamptz;

UPDATE extracted_document_chunks c
   SET occurred_at = f.created_at
  FROM app_drive_files f
 WHERE f.id = c.file_id;

-- The FK to app_drive_files is ON DELETE CASCADE, so every chunk has a
-- file and the backfill left no NULLs.
ALTER TABLE extracted_document_chunks
    ALTER COLUMN occurred_at SET NOT NULL;
