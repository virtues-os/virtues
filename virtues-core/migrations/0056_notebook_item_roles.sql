-- Notebook item roles go live (researcher-plan D1.4).
--
-- The role column has existed since 0032 with DEFAULT 'pin', but nothing ever
-- set it: every member was inserted as 'pin' while scope resolution used ALL
-- members. Now that resolution filters to role='library' (= grounds chat,
-- which is what membership means — the "Library" noun itself is retired),
-- existing members must be backfilled or every current notebook would
-- silently lose its scope.
UPDATE app_notebook_items SET role = 'library' WHERE role = 'pin';

-- New inserts default to grounding too (was 'pin').
ALTER TABLE app_notebook_items ALTER COLUMN role SET DEFAULT 'library';
