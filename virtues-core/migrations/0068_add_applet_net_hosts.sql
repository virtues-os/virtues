-- 0068_add_applet_net_hosts
--
-- The hosts the owner lets an applet's code reach. Set only on the applet's
-- page (PUT /api/applets/:id/hosts), never from the applet's file or the
-- applet tool. A run reaches a host only when it is here AND listed as
-- net(host) in the applet's allowed-tools.
ALTER TABLE app_applets ADD COLUMN net_hosts text[] NOT NULL DEFAULT '{}';

-- Every request a sandboxed run made through the egress proxy, allowed or
-- refused, so the run's page can say what it reached.
CREATE TABLE app_applet_run_requests (
    id bigserial PRIMARY KEY,
    run_id text NOT NULL REFERENCES app_applet_runs(id) ON DELETE CASCADE,
    host text NOT NULL,
    method text NOT NULL,
    sent_bytes bigint NOT NULL DEFAULT 0,
    received_bytes bigint NOT NULL DEFAULT 0,
    refused_reason text,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_applet_run_requests_run ON app_applet_run_requests (run_id);
