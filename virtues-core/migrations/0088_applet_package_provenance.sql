-- 0088_applet_package_provenance
--
-- Where an installed package came from.
--
-- Nothing recorded this. `applet_git_import` resolved the commit SHA after
-- cloning, returned it in the HTTP response, and dropped it — the TS client
-- does not even declare the field. So the box could not answer "where did this
-- applet come from" or "is there a newer version", which makes the uniform
-- update story the package model is *for* impossible to build.
--
-- One row per installed package rather than columns on `app_applets`: the
-- package is the unit that gets installed, updated and removed, and reconcile
-- has no business knowing about git. An applet is tied to its package by its id
-- prefix (`applet_<slug>`), the same key the importer diffs on.
--
-- `forked_from` deliberately does NOT live here — it is stamped into the forked
-- manifest itself so it survives a database rebuild and travels with the folder
-- if it is ever committed. See applet_templates::fork_applet.

CREATE TABLE app_applet_package (
    -- Folder under the state root. Derived from host + owner + repo, so two
    -- different remotes can never claim the same package.
    slug            TEXT PRIMARY KEY,
    repo_url        TEXT NOT NULL,
    -- The ref as requested: a branch, a tag, or a commit.
    git_ref         TEXT NOT NULL,
    -- The commit actually checked out. Null only if rev-parse failed.
    commit_sha      TEXT,
    imported_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE app_applet_package IS
    'Provenance for git-installed applet packages: where each slug came from and what commit is on disk.';
