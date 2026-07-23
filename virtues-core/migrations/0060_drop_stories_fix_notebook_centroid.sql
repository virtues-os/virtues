-- Drop the `wiki_stories` claim tables, and fix the notebook magnet's centroid.
--
-- STORIES, CUT. 0038 split notebooks from stories and built `wiki_stories` as
-- a claim primitive ("I got fat"; "I act differently on rainy days") whose body
-- would be a rendered, cited account. That rendering was never built, and the
-- feature is cut from v1: a spike against real data (2026-07-22) could not name
-- who it helps, and the plumbing beneath it needed real work before the value
-- was ever proven. The tables held nothing but that spike's throwaway rows.
-- Nothing in the app reads them (the /stories route and its placeholder view
-- are removed in the same change), so they go.
--
-- THE NOTEBOOK MAGNET WAS DEAD. The magnet — a notebook that fills itself, the
-- one primitive notebooks and stories shared — never attached a single member
-- on any box. 0039 declared `centroid halfvec(256)` on a Matryoshka-256 theory,
-- but the deployed embedder is gte-small at 384 and `search_vectors.embedding`
-- is halfvec(384). Every centroid write failed the dimension check; it never
-- surfaced only because the column was NULL everywhere, so nothing ever tried
-- until the spike turned the magnet on. One vector type across the path, checked
-- by the database instead of a stale comment.

-- 1. Stories: drop the claim tables. -----------------------------------------
-- `wiki_story_members` FKs `wiki_stories(id) ON DELETE CASCADE`; drop it first.
DROP TABLE IF EXISTS wiki_story_members;
DROP TABLE IF EXISTS wiki_stories;

-- 2. Notebook centroid: 256 → 384, matching the embedder and search_vectors. --
-- The column is NULL everywhere (nothing ever wrote a centroid), so this
-- converts no data. `<=>` now compares halfvec(384) to halfvec(384) with no
-- cast, and the dimension is enforced by pgvector.
ALTER TABLE app_notebooks
    ALTER COLUMN centroid TYPE halfvec(384);
