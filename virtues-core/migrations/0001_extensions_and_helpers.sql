-- 0001 — Extensions and shared helpers.
--
-- This is the first migration the appliance ever runs. Everything downstream
-- (vector columns, updated_at triggers) depends on it.

CREATE EXTENSION IF NOT EXISTS vector;

-- One PL/pgSQL function backs every updated_at trigger in the schema.
-- Each table just attaches it with:
--   CREATE TRIGGER set_updated_at BEFORE UPDATE ON foo
--     FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();
CREATE OR REPLACE FUNCTION tg_set_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;
