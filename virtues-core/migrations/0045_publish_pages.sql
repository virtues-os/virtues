-- 0045_publish_pages
--
-- Pages publish through the door too (api::publications), so a publication's
-- producer may be a page as well as an applet. The page's text is frozen to
-- standalone HTML at share time, like an applet face.
--
-- app_page_shares, the old page-only share that built links from the app's
-- own origin, is no longer read or written. The table stays until no release
-- that reads it can be rolled back to; dropping it is a later migration.

ALTER TABLE app_publications DROP CONSTRAINT IF EXISTS app_publications_producer_kind_check;
ALTER TABLE app_publications
    ADD CONSTRAINT app_publications_producer_kind_check CHECK (producer_kind IN ('applet', 'page'));
