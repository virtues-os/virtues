-- A colour for each pin — the sidebar's ribbon.
--
-- Stores a TOKEN KEY, never a hex value: 'orange', 'emerald', 'violet'. The UI
-- resolves it to var(--cat-<key>), the categorical palette in themes.css that
-- already carries a light set and a nine-theme dark override. A hex would be
-- correct in exactly one of the sixteen themes and wrong in the rest — the
-- pin someone coloured while on Caladan would still be #f97316 on Borghese,
-- which is a monochrome theme by design.
--
-- This is also the one place in the app where non-semantic colour is allowed.
-- The standing rule exists so the *system* can't assert meaning through hue
-- (no badge-blue for a value that isn't blue). A pin's colour is the owner's
-- own index of their own shortcuts — it means "mine", not "the system says" —
-- which is precisely the case the rule was protecting.
--
-- Nullable: an uncoloured pin is the default and renders no ribbon.

ALTER TABLE app_pins ADD COLUMN IF NOT EXISTS color TEXT;
