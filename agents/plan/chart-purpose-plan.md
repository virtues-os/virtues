# Chart purpose: many charts without a long tool description

## What this is for

The dot labs designed 27 inline chat charts, each named by the story its data
tells (bar and line chart, baseline band, interrupted time series, state
timeline, lollipop timeline and so on). The designs, demos and per-chart
contracts live outside this repo, in the owner's design folder
`~/Documents/virtues-dot-labs/charts/` (`simple.json` holds each chart's name,
description, when and how; `entries/*.json` holds its contract and the show.rs
change it needs).

Today `show` knows two chart marks. `show_tool` in
`crates/virtues-registry/src/tools.rs` is about 250 words, and
`virtues-core/src/tools/show.rs` accepts `mark` `bar` or `line` only. Writing
all 27 contracts into the tool description would add roughly 2,000 to 3,000
tokens to every turn that offers the tool, and the model would read all of
them to pick one.

This plan keeps the description short. The model says what the chart is for,
the server picks the chart from the shape of the query's result, and the full
contract reaches the model only when it needs it.

## Who it serves

- **Narrative seeker:** the reply's sentence and its chart still come from the
  same rows, so the words stay exact. The chart fits the story the reply tells.
- **Quantified self:** more chart types for their own data, picked to fit the
  data's shape, with no extra wait.
- **Tech prosumer:** the full set of charts, under their real names.
- **Recovery:** no change in what is drawn. No chart adds counts, streaks or
  targets.
- **Data sovereign:** See the rows stays under every chart. Nothing new leaves
  the server: the picker runs in show.rs on rows the box already read.

## The design

### 1. A `purpose` argument on `show`

`show` gains `purpose`, one of eleven values. Each names the story the data
tells, in words a model can match to the owner's question:

| `purpose` | The story | Marks it can draw |
|---|---|---|
| `change_over_time` | how a measure moves over time | `bar`, `line`, `area`, `cumulative` |
| `before_and_after` | a dated change, or what happens around a repeated event | `interrupted`, `aligned` |
| `usual_range` | what is normal and how far values stray from it | `baseline`, `dotplot` |
| `compare_groups` | one measure across groups or two moments | `multiples`, `dumbbell`, `list` |
| `part_to_whole` | how a total divides | `stack`, `share`, `waterfall` |
| `relationship` | whether two measures move together | `scatter` |
| `daily_rhythm` | the shape of the day across many days | `states`, `ridgeline`, `diurnal`, `actogram` |
| `events` | what happened or is due, and when | `timeline`, `swimlanes`, `lollipop`, `itinerary` |
| `place` | where | `map`, `route`, `density` |
| `in_sentence` | a figure inside the sentence | `numbers`, `spark` |
| `table` | rows to read, not a shape to see | `table` |

`choices` stays as it is: it takes no SQL and has no purpose.

`kind` and `mark` stay as optional arguments. A call with `kind: "chart",
mark: "line"` works exactly as today, so nothing that calls `show` now breaks.
A call that gives `purpose` and leaves `mark` out lets the server choose. A
call that gives both is drawn with the mark it names, if the rows fit it.

The argument is called `purpose` because it is the word a model reaches for
when it asks itself what the chart is for. `story` reads as narrative, and
`chart_story` repeats the tool's own job.

### 2. One table of marks, read three ways

A single Rust table in `crates/virtues-registry` holds every mark:

```rust
pub struct ChartMark {
    pub mark: &'static str,       // "baseline"
    pub name: &'static str,       // "Baseline band"
    pub purpose: &'static str,    // "usual_range"
    pub kind: &'static str,       // "chart" | "map" | "timeline" | "numbers"
    pub line: &'static str,       // one line for the catalog
    pub how: &'static str,        // the full contract, as simple.json's "how"
    pub max_rows: usize,
}
```

Three things read this table, so the description, the guide and the refusals
can never drift apart:

- **`show_tool`'s description**, built from the table: the eleven purposes,
  then one line per mark (name, mark and what it shows). The target is under
  600 tokens. This replaces today's `kind` description.
- **The `chart_guide` tool** (part 4), which returns one mark's `how`.
- **show.rs's refusals**, which quote a mark's `how` when its rows don't fit.

`ToolConfig.llm_description` is a `String`, so `show_tool()` can build it from
the table at startup.

### 3. The server picks the mark from the result's shape

show.rs runs the query, then picks the mark within the purpose from the column
names and types. The contracts already give most marks distinct columns, so
most picks need nothing more than that:

| `purpose` | Rows look like | Mark |
|---|---|---|
| `change_over_time` | a `step` column, or a `step_kind` argument | `cumulative` |
| | a date or time first, then 1 to 3 numbers | `line` |
| | a label first, then 1 to 3 numbers | `bar` |
| `before_and_after` | `event`, `offset` and a value | `aligned` |
| | a `split_at` argument | `interrupted` |
| `usual_range` | a value with `usual_low` and `usual_high` | `baseline` |
| | one value column, with an optional label | `dotplot` |
| `compare_groups` | `panel`, x and a value | `multiples` |
| | a category, then exactly two numbers | `dumbbell` |
| | `label` and `value` | `list` |
| `part_to_whole` | x, then 2 to 5 numbers | `stack` |
| | `label` and `amount`, with a negative amount or an opening first row | `waterfall` |
| | `label` and `value`, all positive, 2 to 8 rows | `share` |
| `daily_rhythm` | `day`, `start`, `end` and `state` | `states` |
| | `day`, `hour` or `minute`, and a value | `ridgeline` |
| | `hour`, `days` and `share` | `diurnal` |
| | `day`, `started_at` and `ended_at` | `actogram` |
| `events` | `lane`, `at` and `what` | `swimlanes` |
| | `due`, `label` and an optional `amount` | `lollipop` |
| | `place`, `started_at` and `ended_at` | `itinerary` |
| | `start`, an optional `end`, and `label` | `timeline` |
| `place` | `lat`, `lon` and `at` | `route` |
| | `lat`, `lon` and `minutes` | `density` |
| | `lat`, `lon` and an optional `label` | `map` |

Ties are broken in the order the table lists the rows. A few marks share a
shape and are reached only by naming them: `area` (same rows as `line`) and
`scatter`'s weekly-rhythm form. When no row fits, the refusal names the
purpose's marks with each one's columns, taken from the mark table, so the
retry has the fix in front of it.

The picker is a pure function of the purpose, the call's arguments and the
column names and types. It gets unit tests: one for each row of the table
above, plus each tie.

### 4. `chart_guide(mark)`, for the few marks with extra arguments

A small read-only tool returns one mark's `how` from the table. The model calls
it only when it wants a mark that takes call arguments beyond the SQL:

- `aligned` takes `event_label`.
- `interrupted` takes `split_at` and `split_label`.
- `cumulative` takes `step_kind` and `unit`.
- `lollipop` takes `days`.
- `density` takes `period`.

The catalog line for each of these says "see chart_guide", so the model knows
to ask. Everything else is learned from a refusal, which costs a retry only
when the model guessed wrong.

### 5. Row caps per mark

Several marks draw more than 200 rows: `ridgeline` and `aligned` up to 1,200,
`states` up to 600, `actogram`, `dotplot`, `scatter`, `multiples`,
`swimlanes` and `density` up to 400.
`SqlQueryTool::execute` clamps every limit to 200
(`virtues-core/src/tools/sql_query.rs:100`, `args.limit.unwrap_or(50).min(200)`),
so show.rs would silently draw a cut result. show.rs needs its own path to the
executor that takes the mark's `max_rows + 1` as its limit: a crate-internal
`SqlQueryTool::query_for_show`. It keeps sql_query's role, timeout and
64 KiB byte budget. `Kind::max_rows` becomes `ChartMark.max_rows`.

## What this does not do

- **No sub-agent to make charts.** A second model that writes the SQL and
  picks the chart would have to hand its rows back to the main model, or the
  reply's sentence and its chart would describe different numbers. That rule
  is what `show` exists to keep. A second model would also add a call's
  latency to every chart. If a later measurement shows the model failing to
  write the hard queries (the baseline band's rolling usual range, the
  lollipop timeline's recurring payments), a `query_for_chart` helper that
  returns SQL to the main model is the place to try one. Its model comes
  from the slot registry.
- **No client work in this plan.** Each mark still needs its Svelte component
  in `apps/web/src/lib/components/chat/show/`, ported from the lab module.
  Those land one mark at a time, each with its show.rs branch. The picker
  only offers marks the client can draw.

## Order of work

1. **Built.** The mark table, with today's marks only (`bar`, `line`, `map`, `timeline`,
   `numbers`, `table`). Build `show_tool`'s description from it. Add
   `purpose` and the picker for those marks. This ships on its own and
   changes nothing the owner sees.
2. **Built.** `query_for_show` and per-mark row caps.
3. **Three batches built**: baseline, cumulative, interrupted and states; then multiples, dumbbell, list, waterfall, share and stack; then ridgeline, diurnal, actogram, dotplot, scatter and area. Port marks in batches: the client component, the show.rs shape branch, the
   table row and the picker row together, in one change per batch. Start with
   the marks the model will reach for most: `baseline`, `cumulative`,
   `interrupted` and `states`.
4. `chart_guide`, once the first mark with extra arguments lands.

## How to tell it worked

- `show_tool`'s description stays under 600 tokens with every mark listed.
  Measure it with the registry's own token count in a test that fails above
  the limit.
- On a fixed set of chart questions from the lab demos' asks, run against an
  invented-data box, the model reaches a drawn chart in one call, or two at
  most after a refusal.
- No reply quotes a figure that is not in the rows `show` returned. That rule
  is already in `show`'s note, and the eval checks it.
