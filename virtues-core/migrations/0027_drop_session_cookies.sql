-- Collapse device auth to the iroh identity.
--
-- Interactive clients (iOS, desktop + its webview via the daemon, CLI-over-iroh)
-- now authenticate by their cryptographically-proven, allowlisted iroh
-- EndpointId (app_device.node_id) — not a browser session cookie. The cookie
-- path (and its CSRF companion) is removed, so the session table is dead.
--
-- Bearer credentials in `credentials` (webhooks / OAuth / programmatic pushes)
-- are a separate, surviving class and are untouched here.
DROP TABLE IF EXISTS app_auth_session;
