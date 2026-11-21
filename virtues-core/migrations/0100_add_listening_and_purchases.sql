-- 0100_add_listening_and_purchases
--
-- Two records the one-time imports write (agents/plan/more-sources-plan.md,
-- slice 4). Both dedup on `source_stream_id`, so importing a newer export
-- later adds only what is new.
--
-- data_activity_listening: one row per play, from a Spotify data download.
-- A play is a span: Spotify records when it stopped and how long it played,
-- so `started_at` is the stop time less `played_ms`. Spotify also records the
-- IP address and country of each play; neither is kept.
--
-- data_financial_purchase: one row per item ordered, from an Amazon "Request
-- your data" download. Kept apart from data_financial_transaction, which
-- holds what the bank charged: an order and its card charge are two views of
-- one purchase, and keeping them in one table would count it twice. Shipping
-- and billing addresses, payment details and gift messages in the export are
-- not kept.
--
-- The CHECKs are a backstop. Each import validates before inserting, because
-- a CHECK failure rejects the whole batch.

CREATE TABLE public.data_activity_listening (
    id                text PRIMARY KEY,
    started_at        timestamptz NOT NULL,
    ended_at          timestamptz NOT NULL,
    played_ms         bigint NOT NULL,
    kind              text NOT NULL,
    title             text,
    artist_name       text,
    album_name        text,
    uri               text,
    is_skipped        boolean,
    is_shuffle        boolean,
    reason_start      text,
    reason_end        text,
    source_stream_id  text NOT NULL UNIQUE,
    source_table      text NOT NULL,
    source_provider   text NOT NULL,
    deleted_at_source timestamptz,
    is_archived       boolean NOT NULL DEFAULT false,
    metadata          jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT data_activity_listening_kind_check CHECK (kind IN ('track', 'episode', 'audiobook')),
    CONSTRAINT data_activity_listening_played_check CHECK (played_ms >= 0 AND ended_at >= started_at)
);

CREATE INDEX data_activity_listening_started_at_idx ON public.data_activity_listening (started_at);

CREATE TABLE public.data_financial_purchase (
    id                 text PRIMARY KEY,
    occurred_at        timestamptz NOT NULL,
    order_id           text NOT NULL,
    item_name          text NOT NULL,
    external_id        text,
    quantity           integer,
    unit_price_cents   bigint,
    total_cents        bigint,
    currency           text,
    status             text,
    merchant_name      text NOT NULL,
    source_stream_id   text NOT NULL UNIQUE,
    source_table       text NOT NULL,
    source_provider    text NOT NULL,
    deleted_at_source  timestamptz,
    is_archived        boolean NOT NULL DEFAULT false,
    metadata           jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX data_financial_purchase_occurred_at_idx ON public.data_financial_purchase (occurred_at);
