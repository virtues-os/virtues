-- 0076_drop_setup_phrase_and_display
--
-- Virtues no longer sells hardware, and the hardware setup path is deleted:
-- the four-word setup phrase (app_setup_phrase) and the on-server display
-- panel's settings (app_display) have no reader or writer left.
DROP TABLE IF EXISTS public.app_setup_phrase;
DROP TABLE IF EXISTS public.app_display;
