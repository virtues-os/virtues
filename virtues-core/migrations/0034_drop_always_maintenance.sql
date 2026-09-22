-- `wiki_articles.maintenance` had three values and two behaviors.
--
-- The column arrived with CHECK (maintenance IN ('always','auto','never')),
-- and the only reader that has ever existed — `wiki_editor::due_articles` —
-- tests `maintenance <> 'never'`. So 'always' and 'auto' select exactly the
-- same articles, on the same 30-day interval (7 for years and stories), behind
-- the same evidence-drift gate. Nothing in the app can even produce 'always':
-- the column defaults to 'auto', the UI toggle writes 'auto' or 'never', and
-- only a hand-written API call could set the third.
--
-- Two names for one behavior is the thing the columns rule exists to stop: a
-- reader finds 'always' in the constraint, concludes there is an eager tier,
-- and writes a comment or a settings row describing a feature that is not
-- there.
--
-- The alternative was to GIVE 'always' its meaning (revise on the interval
-- whether or not the evidence moved). Nobody wants it: drift is the gate that
-- keeps the editor cheap enough to run hourly, and an article rewritten to say
-- the same thing in new words is worse than one that sits still.
--
-- The default is untouched and stays 'auto'. An article worth writing is worth
-- keeping true, the four gates make that slow and cheap, and every article
-- discloses it in its own colophon with the off switch beside it.

-- No box has produced one of these, but tighten the constraint against what is
-- actually there rather than against what should be.
UPDATE wiki_articles SET maintenance = 'auto' WHERE maintenance = 'always';

ALTER TABLE wiki_articles DROP CONSTRAINT IF EXISTS wiki_articles_maintenance_check;
ALTER TABLE wiki_articles ADD CONSTRAINT wiki_articles_maintenance_check
    CHECK (maintenance IN ('auto', 'never'));
