-- 0064_add_agent_device_kind
--
-- A fifth device kind: `agent`, an MCP bridge the owner runs on their own
-- machine (agents/plan/mcp-plan.md, tier 1). It pairs with its own iroh key
-- like any device, and the auth extractor lets it reach `/api/mcp` and no
-- other route. A pair token can be minted for it, so both checks gain it.

ALTER TABLE app_device DROP CONSTRAINT app_device_kind_check;
ALTER TABLE app_device
    ADD CONSTRAINT app_device_kind_check
    CHECK (kind IN ('mobile_app', 'desktop_app', 'sensor', 'cli', 'agent'));

ALTER TABLE app_pair_token DROP CONSTRAINT app_pair_token_intended_kind_check;
ALTER TABLE app_pair_token
    ADD CONSTRAINT app_pair_token_intended_kind_check
    CHECK (intended_kind IN ('mobile_app', 'desktop_app', 'sensor', 'cli', 'agent'));
