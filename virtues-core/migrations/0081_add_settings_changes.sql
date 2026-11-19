-- 0081_add_settings_changes
--
-- What the owner changed in Settings, for "Changed lately" at the top of the
-- Settings page (agents/plan/settings-search-plan.md). One row per setting
-- that a save changed, with the value before and after, so a change can be
-- undone from the list.
--
-- `setting_id` is the setting's id in apps/web/src/lib/settings ('theme',
-- 'style'); `field` is the stored field the save wrote (app_user_profile's
-- `theme`, app_assistant_profile's `style_notes`), which is what an undo
-- writes back. Values are JSON so one table holds text, numbers, booleans
-- and nulls alike.
--
-- `actor` is 'owner', or 'assistant' when the owner applied a change the
-- assistant drafted. `undone_at` is set when the change was undone; the undo
-- itself is recorded as a change of its own.

CREATE TABLE app_settings_changes (
    id          bigserial PRIMARY KEY,
    setting_id  text NOT NULL,
    field       text NOT NULL,
    old_value   jsonb,
    new_value   jsonb,
    actor       text NOT NULL DEFAULT 'owner'
                CHECK (actor IN ('owner', 'assistant')),
    occurred_at timestamptz NOT NULL DEFAULT now(),
    undone_at   timestamptz
);

CREATE INDEX app_settings_changes_occurred_at_idx
    ON app_settings_changes (occurred_at DESC);
