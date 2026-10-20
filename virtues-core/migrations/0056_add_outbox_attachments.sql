-- 0056_add_outbox_attachments
--
-- Files sent with an outbox message, as references, not bytes: each is
-- {ref, filename, mime_type, size_bytes}, where `ref` is a drive file
-- (/drive/<id>) or an iMessage attachment the record holds
-- (/api/messages/<id>/attachments/<n>). The worker reads the bytes when the
-- message goes, so a scheduled message holds nothing heavy while it waits.

ALTER TABLE app_message_outbox ADD COLUMN attachments jsonb NOT NULL DEFAULT '[]'::jsonb;
