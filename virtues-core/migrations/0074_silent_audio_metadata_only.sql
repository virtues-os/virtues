-- Silent chunks ship metadata only (timestamps + dB levels, no audio bytes)
-- from the phone — uploading ~900KB of measured silence over cellular every
-- 5 minutes was a large share of the iOS battery drain. A silent recording row
-- therefore has no lake object to point at.
ALTER TABLE data_audio_recording ALTER COLUMN audio_url DROP NOT NULL;
