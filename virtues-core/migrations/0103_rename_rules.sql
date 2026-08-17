-- `wiki_standing_order` → `wiki_rules`.
--
-- "Standing orders" is the vocabulary of a chain of command, in a product whose
-- whole argument is that nobody is above the owner. It was also naming the
-- wrong thing: these are not a separate document to be learned about, they are
-- the enforceable subset of the one the person already wrote. A sentence in
-- their own answers, marked as a rule rather than context.
--
-- A rename rather than an edit to 0101 on purpose. Changing a committed
-- migration changes its checksum, and any database that already applied it
-- refuses to start — which is a bad trade for a nicer diff.
ALTER TABLE IF EXISTS wiki_standing_order RENAME TO wiki_rules;

-- Rules that belong to a person, a place or an organization travel with them:
-- the check fires when the box is about to surface that entity, rather than
-- depending on a model remembering a global list. Optional, because plenty of
-- rules are about a topic ("never suggest bars") or a context and have no
-- entity to hang on.
--
-- The list still exists and is still the contract. A rule scattered across four
-- hundred wiki entities is a rule nobody can audit, and unverifiable is exactly
-- what these cannot be — you have to be able to read every rule your box obeys
-- on one screen.
ALTER TABLE wiki_rules
    ADD COLUMN IF NOT EXISTS subject_type TEXT
        CHECK (subject_type IS NULL OR subject_type IN ('person', 'place', 'organization'));
ALTER TABLE wiki_rules
    ADD COLUMN IF NOT EXISTS subject_id TEXT;
