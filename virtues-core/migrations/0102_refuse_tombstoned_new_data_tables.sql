-- 0102_refuse_tombstoned_new_data_tables
--
-- The three data tables added by 0099 and 0100 (photos, listening,
-- purchases) were created without the trigger every data table carries
-- (0089): a row the owner deletes leaves a tombstone, and this trigger keeps
-- the next sync or import from writing it back. The recent-plays sync reads
-- Spotify every 30 minutes and the photo collector resends changes, so
-- without it a deleted play or photo would return.

CREATE TRIGGER refuse_tombstoned BEFORE INSERT ON public.data_content_photo
    FOR EACH ROW EXECUTE FUNCTION public.app_refuse_tombstoned();
CREATE TRIGGER refuse_tombstoned BEFORE INSERT ON public.data_activity_listening
    FOR EACH ROW EXECUTE FUNCTION public.app_refuse_tombstoned();
CREATE TRIGGER refuse_tombstoned BEFORE INSERT ON public.data_financial_purchase
    FOR EACH ROW EXECUTE FUNCTION public.app_refuse_tombstoned();
