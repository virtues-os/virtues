# Demo box

Everything that turns a box into a demo box: one fictional life, placed on
today, reset nightly. The life is `virtues-core/seeds/demo/life.sql.gz`
(see [its README](../../virtues-core/seeds/README.md)).

| File | What it does |
|---|---|
| `install.sh` | Backs up, clears the box's life, loads the demo life, places it, installs the units, turns the day writer off, starts a reindex. Refuses unless the env file says `VIRTUES_DEMO_BOX=1`. |
| `clear.sql` | Empties the life (every `data_*`, `wiki_*` and `search_*` row, chats, pages, projects). Keeps auth, devices, settings, system applets and the box's real weather. |
| `place.sql` | Puts the life on today. Hourly. |
| `reset.sql` | Deletes what a visitor wrote and restores the owner. Nightly at 08:00 UTC. |
| `check.sh` | CI: the life loads on the current migrations and places twice cleanly. |
| `health.sh` | The demo check file for `virtues-health`. |
| `units/` | `virtues-demo-place` (hourly) and `virtues-demo-reset` (nightly). |

## Placement

`demo.rows` lists every row of the life with a bucket. The seven days of the
showcase week (buckets 0–6, Saturday–Friday) each live on the most recent date
at or before today with their own weekday. So weekdays are always true, today
is always a full day, and a date keeps its content for a week, moving only
when its weekday comes round again. The backbone (bucket −1) ends the day
before the week. Week first, backbone second: the week vacates the date the
backbone lands on next.

Only listed rows move, and only the columns that say when the life happened
(`started_at`, `ended_at`, `occurred_at`, `date`, ...). The box's own rows,
real weather and a visitor's chat, never move. "Today" is the box's UTC date.

## The reset

Visitors share the box, so what one writes is otherwise visible to the next.
`reset.sql` deletes product state that is not in `demo.rows`: chats, pages,
projects, memories, pins, marginalia, drive files, and the people, places and
organisations a visitor's chat added. It never names auth, so a reviewer's
pairing survives. It also restores the owner profile, because a visitor who
walks through setup renames the owner and moves the home timezone.

## Locally

`make dev-demo` loads the same life into its own database, `virtues_demo`,
never the shared dev database, and prints how to serve it beside the shared
core. Re-run it to reload.
