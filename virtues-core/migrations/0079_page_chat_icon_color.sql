-- 0079 — A color to go with the icon, for pages and chats.
--
-- `app_pages.icon` and `app_chats.icon` have existed since 0005; the color to
-- draw them in has not, so the icon picker could offer an icon and nothing
-- else. Notebooks already had `accent_color` (0003) and pins got `color`
-- (0070). These two were the gap, and the gap is why the picker would
-- otherwise have to behave differently depending on what opened it.
--
-- Stores a TOKEN KEY, never a hex — 'orange', 'emerald', 'violet' — resolved
-- by the UI to `var(--cat-<key>)`. Same contract as 0070, and the argument has
-- not changed: a hex is correct in exactly one of the sixteen themes, and the
-- icon someone colored on Caladan would still be #f97316 on Borghese, which is
-- monochrome by design. It is also why the picker offers nine swatches and no
-- eyedropper, however much Linear's has one.
--
-- Named `icon_color` rather than `color`: it colors the icon specifically, not
-- the page. A page has no tint, no accent surface and no border — a general
-- "page color" would promise something the UI does not do.
--
-- Nullable, and null is the normal state: an uncolored icon inherits the text
-- color it sits in, exactly as every icon does today. No existing row changes.

ALTER TABLE app_pages ADD COLUMN IF NOT EXISTS icon_color TEXT;
ALTER TABLE app_chats ADD COLUMN IF NOT EXISTS icon_color TEXT;
