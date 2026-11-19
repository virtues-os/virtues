-- 0085_add_units_recordings_and_reading
--
-- Three settings: two on the owner's profile, one on the assistant's.
--
-- `units`: which units the owner reads in (Settings · Profile · Units),
-- 'us' (miles, pounds, °F) or 'metric' (kilometres, kilograms, °C). The app
-- formats the numbers it draws with it, and the chat and the day write-up
-- are told to write in it. Existing boxes keep US units, which is what the
-- app showed.
--
-- `keep_recordings_days`: how long audio recordings are kept (Settings ·
-- Your data · Keep recordings). NULL keeps them forever, the default. 30, 90
-- or 365 has the maintenance sweeper delete the audio of recordings older
-- than that once they are transcribed; the transcripts stay.
--
-- `app_assistant_profile.reading`: what the assistant may read (Settings ·
-- Your data · What the assistant may read), one boolean per kind of record
-- keyed by its id in `api::model_access`. A missing key takes that kind's
-- default: on, except cycle and symptoms, which stay off until turned on.
--
-- Rolling back to an older release: that release does not read these
-- columns, so they are ignored, nothing deletes audio, and only cycle data
-- stays withheld from the assistant.

ALTER TABLE app_user_profile
    ADD COLUMN units text NOT NULL DEFAULT 'us' CHECK (units IN ('us', 'metric')),
    ADD COLUMN keep_recordings_days integer CHECK (keep_recordings_days IN (30, 90, 365));

ALTER TABLE app_assistant_profile
    ADD COLUMN reading jsonb NOT NULL DEFAULT '{}'::jsonb;
