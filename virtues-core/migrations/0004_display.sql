-- 0004_display
--
-- What the box's attached screen shows in its ambient slot — the "face" the
-- panel wears once the box is claimed and nothing is interrupting (Settings →
-- Display). A singleton, like app_user_profile: the box has one screen.
--
-- The face is a fact about the BOX, not about any paired device, which is why
-- this is a table and not a ui_preferences key: the kiosk reads it through the
-- loopback /api/display/state with no session at all.
--
-- face_kind:
--   'builtin' — a face the panel renders itself; face_builtin names it
--               ('record' is the ambient census screen, 'matte' is black glass
--               on purpose — reserve, not off).
--   'applet'  — an applet's face/index.html, rendered in the ambient slot;
--               face_applet_id names the applet.
CREATE TABLE app_display (
    id boolean PRIMARY KEY DEFAULT true,
    face_kind text NOT NULL DEFAULT 'builtin'
        CHECK (face_kind IN ('builtin', 'applet')),
    face_builtin text NOT NULL DEFAULT 'record',
    face_applet_id text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT display_singleton CHECK (id),
    CONSTRAINT display_applet_named CHECK (
        face_kind <> 'applet' OR face_applet_id IS NOT NULL
    )
);

INSERT INTO app_display (id) VALUES (true);
