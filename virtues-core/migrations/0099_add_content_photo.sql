-- 0099_add_content_photo
--
-- Photos from the iPhone's library, one row per photo or video. Every photo
-- gets a row with what PhotoKit says about it: when it was taken, where, what
-- kind it is, its size and whether it is a favorite. Only a favorite also gets
-- a picture: a copy about 1024 pixels wide kept in Drive, named by
-- `asset_id`. Originals stay on the phone. The Hidden album is never read, so
-- a hidden photo has no row. See agents/plan/more-sources-plan.md, slice 3.
--
-- `source_stream_id` is `ios:photo:<PhotoKit local identifier>`, stable for the
-- life of the photo on that phone, so a photo sent again (favorited, edited,
-- moved) updates its row. A photo deleted on the phone keeps its row with
-- `deleted_at_source` set.
--
-- The CHECK is a backstop. ios_ingest validates `kind` before inserting,
-- because a CHECK failure rejects the whole batch and the phone would retry it
-- forever.

CREATE TABLE public.data_content_photo (
    id                text PRIMARY KEY,
    occurred_at       timestamptz NOT NULL,
    kind              text NOT NULL,
    is_favorite       boolean NOT NULL DEFAULT false,
    latitude          double precision,
    longitude         double precision,
    width_px          integer,
    height_px         integer,
    duration_seconds  double precision,
    asset_id          text,
    source_stream_id  text NOT NULL UNIQUE,
    source_table      text NOT NULL,
    source_provider   text NOT NULL,
    deleted_at_source timestamptz,
    is_archived       boolean NOT NULL DEFAULT false,
    metadata          jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT data_content_photo_kind_check
        CHECK (kind IN ('photo', 'live_photo', 'screenshot', 'video'))
);

CREATE INDEX data_content_photo_occurred_at_idx ON public.data_content_photo (occurred_at);
