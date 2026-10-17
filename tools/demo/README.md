# Building the demo life

`python3 tools/demo/build.py` writes `virtues-core/seeds/demo/life.sql.gz`
from two inputs, in a scratch database it drops afterwards:

- **`gen-backbone.py`**: the three-year backbone, generated, deterministic.
  Edit the cast, the chapters or the per-day writers here and rebuild.
- **`week/week.sql.gz`**: the showcase week, frozen. The box wrote its events
  and day pages with model calls, so it is kept as data, not regenerated on
  every build.

Rebuild when a migration breaks the file (CI says so) or when the life should
change. Build with `--schema <migration>` set to the oldest release the demo
box runs (`select max(version) from _sqlx_migrations` there): the file then
loads on that release and every newer one, since new columns take their
defaults. Each rebuild adds about 2.5 MB to the repo's history, so batch changes.

## The showcase week (`week/`)

Made once, from the shape of a real week. Rerun it only to replace the week.

1. `extract.py` reads the shape of a source week (timings, counts, lengths,
   place clusters, move paths as fractions) from a local copy of a real record.
   No text is read into the output.
2. `cast.py`, `geo.py`, `skeleton.py` map that shape onto the backbone's
   persona: their people, their places, routes computed between fictional
   places.
3. `plan.py`, `content.py` write the week as fiction with a model. Prompts
   carry the persona, the cast and the shape, never a real word.
4. `emit.py` writes the raw week as SQL, and `leakcheck.py` checks it against
   the source record: no shared phone, email or full name, no 8-word run of
   shared text (reviewed boilerplate is in `leak_allow.txt`), and no point
   within 300 m of a place with 10+ hours of real dwell.
5. On a demo box: load it, have the box write the days
   (`day_summary_eod` with `config.date` per day; the last day goes through
   `moveday.sql` because the writer refuses a day that is not over), then
   `freeze.sql` and `export-week.sh` produce `week.sql.gz`.

Working files hold real timing and real domains, so they live outside the repo
in `$DEMO_WEEK_WORK` (default `~/virtues-demo-src`) and never in git. Run
`leakcheck.py` on the built `life.sql` before committing a rebuild that
changes either input.
