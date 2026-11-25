-- 0111_add_photo_resolver
--
-- Photo favorites. `data_content_photo` (0099) holds the photos the owner
-- favorited on their iPhone; the phone sends no other photo. This adds what a
-- bookmark has: the owner's note, and the description the picture pass writes
-- so a photo can be found by what is in it. The columns mean what they mean on
-- `data_content_bookmark`.
--
-- A photo that stops being a favorite, or is hidden or deleted on the phone,
-- keeps its row and its note, with `deleted_at_source` set. Its picture, its
-- place and its description are removed.
--
-- The CHECK is a backstop, as in 0099: only the pass writes the status.

ALTER TABLE public.data_content_photo
    ADD COLUMN note                    text,
    ADD COLUMN extraction              jsonb,
    ADD COLUMN extraction_text         text,
    ADD COLUMN enrichment_status       text NOT NULL DEFAULT 'pending',
    ADD COLUMN enrichment_attempts     integer NOT NULL DEFAULT 0,
    ADD COLUMN enrichment_last_attempt timestamptz,
    ADD COLUMN enrichment_model        text,
    ADD COLUMN enriched_at             timestamptz,
    ADD CONSTRAINT data_content_photo_enrichment_status_check
        CHECK (enrichment_status IN ('pending', 'enriching', 'done', 'failed', 'skipped'));

CREATE INDEX data_content_photo_enrichment_queue_idx
    ON public.data_content_photo (occurred_at DESC)
    WHERE enrichment_status IN ('pending', 'enriching');

-- Whether another photo still shows a picture is asked before its file goes.
CREATE INDEX data_content_photo_asset_id_idx
    ON public.data_content_photo (asset_id)
    WHERE asset_id IS NOT NULL;
