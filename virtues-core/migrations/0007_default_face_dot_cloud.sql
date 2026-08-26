-- 0007_default_face_dot_cloud
--
-- The screen's default face becomes Dot Cloud — the night-sky viz applet —
-- instead of the built-in record census (decided 2026-08-26). New singletons
-- get it from the column defaults; the UPDATE moves only rows still on the
-- factory default, so a box whose owner chose a face keeps the choice.
-- (No box has shipped with app_display yet, so today the UPDATE moves
-- every row that exists — dev checkouts.)
--
-- The record screen remains the kiosk's FALLBACK: a face that cannot be
-- hung (applet deleted, token refused, config unreadable) still degrades to
-- the census — the glass always renders (see /display and
-- api::system_display::face_config_or_default).
ALTER TABLE app_display ALTER COLUMN face_kind SET DEFAULT 'applet';
ALTER TABLE app_display ALTER COLUMN face_applet_id SET DEFAULT 'applet_dot_cloud';

UPDATE app_display
   SET face_kind = 'applet',
       face_applet_id = 'applet_dot_cloud',
       updated_at = now()
 WHERE face_kind = 'builtin'
   AND face_builtin = 'record';
