-- 0066_add_connection_credential_kind
--
-- A fifth kind of secret: `connection`, the sign-in for another service's MCP
-- server that the assistant uses as tools (agents/plan/mcp-plan.md). It is
-- not a `source`: nothing syncs from it, so it must not count as a connected
-- source in setup, the sources list, stream health or the chat's context.
-- `key` is the connection's short name, the prefix of its tool names, and is
-- unique among connections.

ALTER TABLE credentials DROP CONSTRAINT credentials_kind_check;
ALTER TABLE credentials
    ADD CONSTRAINT credentials_kind_check CHECK (kind IN ('source', 'box', 'env', 'login', 'connection'));

UPDATE credentials SET kind = 'connection' WHERE kind = 'source' AND id LIKE 'mcp:%';

CREATE UNIQUE INDEX credentials_connection_key_idx ON credentials (key) WHERE kind = 'connection';
