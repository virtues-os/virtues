-- The assistant has one character, and the owner can add notes to it.
--
-- Before this, the owner picked one of four stored personas (Standard,
-- Concierge, Analyst, Coach) or wrote their own, and whichever was picked
-- REPLACED the character lines in the system prompt. The written character
-- (the `default` arm of `prompt::get_persona_guidelines`) was not one of the
-- four, so choosing anything at all switched it off. Choosing Standard
-- replaced it with "no particular personality", which a real box had done.
--
-- Now the character is always present and `style_notes` is added beneath it:
-- how the owner wants to be spoken to, in their own words. NULL = no notes.
-- The `persona` and `personas` columns stay for a release, unread by the
-- prompt, so an older client that still writes them does not fail.
ALTER TABLE app_assistant_profile ADD COLUMN style_notes TEXT;

-- Carry a deliberate choice across, so nobody's assistant changes voice without
-- being told: whatever text their chosen persona held becomes their notes.
-- Standard, and the ids that meant the written character, carry nothing. Those
-- boxes get the character back, which is the point of the change.
--
-- Persona text was written ABOUT the owner ("Help {user_name} think through
-- problems"); notes are written BY them, and they read this box. So the
-- placeholder becomes "me", or "my" where it was possessive: "Help me think
-- through problems", "invested in my growth". The prompt still
-- substitutes `{user_name}` for any text that keeps one.
UPDATE app_assistant_profile p
SET style_notes = (
    SELECT NULLIF(btrim(replace(replace(item->>'content', '{user_name}''s', 'my'), '{user_name}', 'me')), '')
    FROM jsonb_array_elements(
        CASE WHEN jsonb_typeof(p.personas->'items') = 'array'
             THEN p.personas->'items' ELSE '[]'::jsonb END
    ) AS item
    WHERE item->>'id' = p.persona
    LIMIT 1
)
WHERE p.persona IS NOT NULL
  AND p.persona NOT IN ('standard', 'default', 'capable_warm');
