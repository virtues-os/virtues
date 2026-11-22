# Authoring applets

"Applet" is the code's word and never appears on screen. People see three
words. A **site** is something with a view: a page they open and look at. A
**task** is work that gets done: a prompt that starts on its own is a task
the assistant does, and a program the server runs is one of the server's
tasks. A **skill** is a prompt they start from chat, or that the assistant
loads on its own. In any name, description or reply a person reads, use the
word that fits what you made.

An applet is a short file, `SKILL.md` in the
[Agent Skills](https://agentskills.io/specification) format: a name, a
one-sentence description, and optionally when it starts, a SQL check that
runs first, and a prompt. The design is
[agents/plan/applet-shape.md](../agents/plan/applet-shape.md).

The in-box assistant writes applets with the `applet` tool's `setup` action.
Its description is the whole instruction the assistant gets (`APPLET_ACTIONS`
in [crates/virtues-registry/src/tools.rs](../crates/virtues-registry/src/tools.rs)).
Setup's parameters are the file's keys by the same names, so this page and
that instruction describe one thing. `virtues applet put <folder>` sends a
folder through the same setup.

## The keys

| Setup parameter | In the file | What it is |
|---|---|---|
| `name` | `name` and the folder | a few words; `Water log` is `user/water-log/`. The same name is the same applet, a built-in one included |
| `description` | `description` | one sentence the owner reads: what it does for them, and when to use it |
| `prompt` | the body | what to read, what to write, how long; under 300 words. It runs with no chat history |
| `trigger` | `metadata.trigger` | `every day at 07:00`, `every monday at 09:00`, `every hour`, `on 2026-11-02 at 09:00`, `cron 0 0 7 * * *`, in the box's time; or on rows in a data table: `new data_health_workout`, `30 minutes before data_calendar_event`, `15 minutes after data_calendar_event ends` |
| `run` | `metadata.run` | `sql …` that returns a row only when it should run; no rows skips the prompt. With a data trigger it reads the rows as `new_rows`, and the prompt is given them as a table, beside what `run` returned |
| `invoke` | `metadata.invoke` | `assistant` (the default) lets the assistant start it; `chat` puts it in the `/` menu |
| `tools` | `allowed-tools` | narrows the tools its prompt has, from chat's ordinary set |
| `tables` | `tables/NNNN_schema.sql` | every `CREATE TABLE` it should have, every time; the box adds what is missing and never drops |
| `view` | `view.html` | one HTML page that links `virtues.css` and `virtues.js` and reads with `await virtues.query(sql)` in `<script type="module">` |
| `limit` | `metadata.limit` | `$0.25 per run, $1 per day, 10 minutes`; an applet with a prompt gets `$1 per day` when it has no daily limit, and one with a data trigger gets `6 runs per hour` when it has no hourly limit |
| `model` | `metadata.model` | `standard`, `deep` or `lite`; an unattended prompt starts on `lite` |

An applet the assistant makes starts on. Its tables are in schema
`applet_<name with underscores>`. Its prompt, its `run` and its view name
them without the schema, as `drinks`, and its prompt writes them with
`sql_write`. A chat that loads its prompt with `use_applet` is told the
schema to add.

## Examples

A report, every Sunday evening:

```json
{ "action": "setup", "name": "Week in review",
  "description": "Looks back over your week every Sunday. Use when you ask how your week went.",
  "trigger": "every sunday at 18:00",
  "prompt": "Read the last seven days of data_calendar_event and data_health_sleep. Write a page of three short paragraphs: what took the time, how rest went, and one thing worth noticing. Title it with the date range." }
```

A tracker with a table and a view:

```json
{ "action": "setup", "name": "Water log",
  "description": "Counts the glasses of water you drink. Use when someone says they drank water.",
  "invoke": ["chat", "assistant"],
  "tables": "CREATE TABLE drinks (id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text, ml INTEGER NOT NULL, drank_at TIMESTAMPTZ NOT NULL DEFAULT now());",
  "prompt": "Insert one row into drinks for what they drank (250 ml a glass unless they say), then say today's total.",
  "view": "<!doctype html>…" }
```

Its view, written out. The call carries it as one string. It links the two
runtime files, reads its own table with no schema, and draws with nothing
from another host:

```html
<!doctype html>
<html>
<head>
<meta charset="utf-8">
<link rel="stylesheet" href="virtues.css">
<script src="virtues.js"></script>
<style>
  body { margin: 24px; color: var(--color-foreground); }
  .bars { display: flex; align-items: flex-end; gap: 8px; height: 160px; }
  .bar { flex: 1; background: var(--color-accent); border-radius: 4px 4px 0 0; }
  .days { display: flex; gap: 8px; font-size: 12px; color: var(--color-foreground-subtle); }
  .days span { flex: 1; text-align: center; }
</style>
</head>
<body>
<h1>Water this week</h1>
<p>Milliliters of water you logged on each of the last seven days.</p>
<div class="bars" id="bars"></div>
<div class="days" id="days"></div>
<script type="module">
  const rows = await virtues.query(`
    select to_char(d, 'Dy') as day, coalesce(sum(ml), 0) as ml
      from generate_series(current_date - 6, current_date, interval '1 day') d
      left join drinks on drank_at::date = d::date
     group by d order by d`);
  const most = Math.max(250, ...rows.map((r) => r.ml));
  for (const r of rows) {
    const bar = document.createElement('div');
    bar.className = 'bar';
    bar.style.height = `${(r.ml / most) * 100}%`;
    bar.title = `${r.day}: ${r.ml} ml`;
    document.getElementById('bars').append(bar);
    const label = document.createElement('span');
    label.textContent = r.day;
    document.getElementById('days').append(label);
  }
</script>
</body>
</html>
```

A one-off reminder:

```json
{ "action": "setup", "name": "Dentist", "description": "Reminds you about the dentist.",
  "trigger": "on 2026-11-02 at 09:00", "prompt": "Remind them their dentist appointment is at 11." }
```

A check with no model call on most days:

```json
{ "action": "setup", "name": "Workout nudge", "description": "Nudges you when three days pass without a workout.",
  "trigger": "every day at 09:00",
  "run": "sql select 1 where not exists (select 1 from data_health_workout where started_at > now() - interval '3 days')",
  "prompt": "Write one short line suggesting a workout today." }
```

A check on new data, with no model call unless a reading is high:

```json
{ "action": "setup", "name": "High heart rate", "description": "Tells you when your heart rate runs high.",
  "trigger": "new data_health_heart_rate",
  "run": "sql select occurred_at, bpm from new_rows where bpm > 120",
  "prompt": "Write one short line saying when it was high and how high." }
```

## What holds for any applet

- **Name only tables that exist.** Setup checks every `data_*` and `wiki_*`
  table a prompt names. Columns are not checked, so take them from a real
  `sql_query` sample.
- **Say when it cannot be done.** A prompt has chat's tools and nothing more:
  it cannot fetch a URL or send a message. Offer what is possible, such as a
  tracker the owner logs into, or a `web_search` labeled as one.
- **Make sure, then make.** A digest's prompt says "make sure today's page
  exists; check first, create it if missing", so a missed or repeated run
  does no harm.
- **A daily applet catches up once.** A box asleep at 07:00 runs the 07:00
  applet when it wakes, the same day. Faster schedules wait for the next
  tick.
- **A threshold stays true.** "No workout in 3 days" fires every day of the
  lapse. To fire once, add a cooldown on its own runs to `run`:
  `and not exists (select 1 from app_applet_runs r where r.applet_id = 'applet_user__workout_nudge' and r.status = 'success' and r.started_at > now() - interval '1 day')`.
- **A data trigger takes what came since its last run.** Each run reads every
  row that arrived since the one before, as `new_rows`; a `run` that reads
  the table itself sees every row and is refused. Rows already there
  when it is made start nothing, and neither do new rows about more than a
  week ago, such as the history a new source brings in. `before` and
  `after … ends` count from each row's own start or end.
- **Two cadences are two applets**, each restating what it needs.
- **No persona applets.** "Reply to Mom as me" is not something an applet can
  do; offer one that drafts replies as pages for the owner to send.

- **A view is a whole page, written in one document.** Make one when the
  person asks to see or keep track of something. It opens beside the chat and
  they keep it, so give it a title, a sentence that says what the data shows,
  and then the chart or table. A view cannot reach any other host: a
  stylesheet, script, font or image from a CDN never arrives, and setup
  refuses a view that names one. Write the styles and scripts in the
  document and draw charts with inline SVG or canvas.
- **A view's script is `<script type="module">`.** `await` works at the top
  of a module. In a plain `<script>` it is a syntax error unless it sits
  inside an `async` function, the whole script stops, and the page stays
  empty. Setup refuses a plain script that awaits with no `async` in it.
- **A view shows what was recorded.** Dates, values, and a chart of them. It
  has no streaks, no count toward a goal and no "days since", because someone
  in recovery may be the one reading it.

An example that ships: `applets/morning-examen/` (a scheduled prompt with a
limit). No site ships with the server; the water log's view above is the
example of one.

## For builtin (Rust) applet development

See [AUTHORING.md](./AUTHORING.md) — the subprocess contract, the `SKILL.md`
reference for applets that run code, and the reconcile field-ownership rules.
