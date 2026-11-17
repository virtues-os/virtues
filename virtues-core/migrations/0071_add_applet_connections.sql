-- 0071_add_applet_connections
--
-- The connections the owner lets an applet use, by short name (a
-- `credentials` row of kind `connection`, keyed by that name). Set only on
-- the applet's page (PUT /api/applets/:id/connections), never from the
-- applet's file or the applet tool. A run gets a connection's tools only when
-- the connection is here AND its file lists them in allowed-tools, as
-- `linear__*` or `linear__save_comment`.
ALTER TABLE app_applets ADD COLUMN connections text[] NOT NULL DEFAULT '{}';
