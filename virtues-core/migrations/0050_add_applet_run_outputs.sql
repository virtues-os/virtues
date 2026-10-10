-- 0050_add_applet_run_outputs
--
-- What each applet run made. The runner records one row when a run writes a
-- page (create_page, edit_page, revise_article), so an applet's page can show
-- its pages and a page can say which applet wrote it. Without it the only
-- trace of a run's work was the sentence it ended on.
--
-- `ref_id` names the thing made; it is not a foreign key because a page can be
-- deleted while the run that wrote it stays in the history. Readers join to
-- app_pages and drop what is gone.

CREATE TABLE app_applet_run_outputs (
    run_id     text NOT NULL REFERENCES app_applet_runs(id) ON DELETE CASCADE,
    kind       text NOT NULL CHECK (kind IN ('page')),
    ref_id     text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, kind, ref_id)
);

CREATE INDEX idx_app_applet_run_outputs_ref ON app_applet_run_outputs (kind, ref_id);
