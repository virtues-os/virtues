-- Calendar provenance and RSVP — the fields that separate a PLAN from a RECORD.
--
-- A calendar entry is an intention. Read as evidence of attendance it produces
-- confident fiction: a "Community Dinner" off a subscribed parish calendar was
-- narrated as something the owner did, wearing sensory detail (a piano, a dog)
-- borrowed from an ambient recording made at home during the same hours. The
-- contradicting evidence was in the same dossier; nothing in the schema let the
-- reader tell a plan from a trace.
--
-- Two things were missing. One column was never added, and three were added in
-- 0007 and then never written by either calendar sync.

-- ---------------------------------------------------------------------------
-- 1. WHOSE CALENDAR IS THIS?
-- ---------------------------------------------------------------------------
-- Google's calendarList returns `accessRole` per calendar:
--   owner | writer   → the owner's own calendar; the event is their intention
--   reader           → SUBSCRIBED. Someone else's plans, visible to the owner.
--   freeBusyReader   → subscribed, and we cannot even see the title.
--
-- A `reader` event is never a claim about where the owner was, no matter how
-- specific it looks. This is the cleanest attendance discriminator available and
-- it lives on the CALENDAR, not the event — one field, fetched on every sync.
--
-- NULL means "unknown": an iOS/EventKit row (EventKit exposes no equivalent), or
-- a Google row synced before this migration whose calendar has not been re-listed
-- yet. NULL must NEVER be read as `own`. An unknown provenance is unknown, and
-- consumers omit the claim rather than guess.
ALTER TABLE data_calendar_event ADD COLUMN calendar_access_role TEXT;

COMMENT ON COLUMN data_calendar_event.calendar_access_role IS
    'owner | writer | reader | freeBusyReader. reader/freeBusyReader = a subscribed calendar: someone else''s plans. NULL = unknown provenance — never assume ownership.';

-- ---------------------------------------------------------------------------
-- 2. BACKFILL THE THREE COLUMNS THAT WERE ALWAYS COLLECTED AND NEVER PROJECTED
-- ---------------------------------------------------------------------------
-- `response_status`, `attendee_identifiers` and `organizer_identifier` have
-- existed since 0007 and no writer has ever populated them, so `response_status`
-- has been uniformly NULL and `attendee_identifiers` uniformly '[]' — which is
-- why the "with <attendees>" clause in the dayline context builder has never once
-- fired.
--
-- But the Google transform HAS been stashing the raw `attendees`, `organizer` and
-- `creator` blobs into `metadata` on every write. So this is a projection of data
-- already on the box, not a re-sync: every historical Google event carries the
-- answer and only needed to be asked.
--
-- iOS/EventKit rows cannot be backfilled — that collector never captured
-- participants at all — so they stay NULL until the collector is taught to send
-- them. NULL here is honest: it means "not observed", never "did not attend".

-- 2a. The owner's own RSVP. Google marks exactly one attendee `self: true`.
--
-- A caution for every reader of this column: `needsAction` is the overwhelmingly
-- common value and it means almost nothing — most personal events are
-- self-created with no attendee list, so there is no RSVP to derive and the
-- column stays NULL. The signal here is ONE-DIRECTIONAL: `declined` is strong
-- evidence of absence; `accepted` is weak evidence of presence (it records a
-- decision made weeks earlier, not a body in a room). Absence of a value is not
-- evidence of anything.
UPDATE data_calendar_event
SET response_status = (
        SELECT a ->> 'responseStatus'
        FROM jsonb_array_elements(metadata -> 'attendees') AS a
        WHERE (a ->> 'self')::boolean IS TRUE
          AND a ->> 'responseStatus' IS NOT NULL
        LIMIT 1
    )
WHERE source_provider = 'google'
  AND response_status IS NULL
  AND jsonb_typeof(metadata -> 'attendees') = 'array';

-- 2b. Who else was invited. A better attendance signal than the RSVP: a
-- forty-person parish event and a two-person coffee carry completely different
-- semantics for one person's absence. Rooms and equipment (`resource: true`) are
-- not people and are excluded.
UPDATE data_calendar_event
SET attendee_identifiers = COALESCE(
        (
            SELECT jsonb_agg(ident)
            FROM (
                SELECT COALESCE(a ->> 'email', a ->> 'displayName') AS ident
                FROM jsonb_array_elements(metadata -> 'attendees') AS a
                WHERE COALESCE((a ->> 'resource')::boolean, FALSE) IS FALSE
                  AND COALESCE(a ->> 'email', a ->> 'displayName') IS NOT NULL
            ) AS people
        ),
        '[]'::jsonb
    )
WHERE source_provider = 'google'
  AND attendee_identifiers = '[]'::jsonb
  AND jsonb_typeof(metadata -> 'attendees') = 'array';

-- 2c. Who called it. An event organised by someone else, on a calendar the owner
-- merely reads, is about as far from "a thing the owner did" as a row can get.
UPDATE data_calendar_event
SET organizer_identifier = COALESCE(
        metadata -> 'organizer' ->> 'email',
        metadata -> 'organizer' ->> 'displayName'
    )
WHERE source_provider = 'google'
  AND organizer_identifier IS NULL
  AND jsonb_typeof(metadata -> 'organizer') = 'object';
