-- 0047_publish_chats
--
-- A chat can be shared read-only through the door (api::publish_chat): its
-- visible conversation, frozen to one HTML file. Widens producer_kind again.

ALTER TABLE app_publications DROP CONSTRAINT IF EXISTS app_publications_producer_kind_check;
ALTER TABLE app_publications
    ADD CONSTRAINT app_publications_producer_kind_check CHECK (producer_kind IN ('applet', 'page', 'chat'));
