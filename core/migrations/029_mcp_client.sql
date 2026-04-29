-- MCP Client Support
-- Adds app_mcp_servers (connection config) and app_mcp_tools (discovered tools)
-- Also adds /tools sidebar item to the Connections section

-- MCP server connections
CREATE TABLE IF NOT EXISTS app_mcp_servers (
    id                TEXT PRIMARY KEY,
    name              TEXT NOT NULL,
    url               TEXT NOT NULL,
    description       TEXT,
    auth_token        TEXT,
    enabled           INTEGER NOT NULL DEFAULT 1,
    status            TEXT NOT NULL DEFAULT 'disconnected'
                      CHECK (status IN ('disconnected', 'connecting', 'connected', 'error')),
    last_error        TEXT,
    last_connected_at TEXT,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TRIGGER IF NOT EXISTS app_mcp_servers_set_updated_at
    AFTER UPDATE ON app_mcp_servers
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE app_mcp_servers SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- Drop legacy app_mcp_tools created at startup (different schema, no server_id FK)
DROP TABLE IF EXISTS app_mcp_tools;

-- Discovered tools from connected MCP servers
CREATE TABLE IF NOT EXISTS app_mcp_tools (
    id           TEXT PRIMARY KEY,
    server_id    TEXT NOT NULL REFERENCES app_mcp_servers(id) ON DELETE CASCADE,
    server_name  TEXT NOT NULL,
    tool_name    TEXT NOT NULL,
    description  TEXT,
    input_schema TEXT,
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_app_mcp_tools_server ON app_mcp_tools(server_id);

-- Add /tools to the Connections sidebar section
INSERT OR IGNORE INTO app_space_items (view_id, url, sort_order)
VALUES ('view_sys_sec_data', '/tools', 10);
