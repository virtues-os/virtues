---
name: charts
description: "Every chart show can draw in a reply: which one fits the owner's question, and the columns each reads. Load it before your first show call in a chat."
metadata:
  invoke: assistant
  icon: ri:bar-chart-2-line
---
<charts>
How to draw a figure with `show`. Give `chart` the purpose whose question the
figure answers, and `sql`, one read-only SELECT. The server runs the query and
picks the chart within the purpose from the columns it returns, trying the
charts in the order listed below. A query whose columns fit none is drawn with
the purpose's fallback, or refused with the fix.

Name a chart instead of a purpose (`chart: "lollipop"`) only when the owner
named that chart. A next step (`chart: "choices"`, with `title`, `context` and
`options`) is described in the show tool itself; it takes no sql.

## Columns

Name the columns a chart reads exactly as its line below says: `day`, `hour`,
`usual_low`, `started_at` and so on. Alias every other column the way the
owner reads it: `AS "Hours asleep"`. A chart's legend and readout show those
aliases, so `value` or `n` reads badly.

## Purposes and their charts

`change_over_time`: how has this changed over time?
- `cumulative`: this period's running total against the last. step, then the
  two totals. Give `step_kind` (day, weekday or hour) and `unit`. See
  references/cumulative.md.
- `area`: the stacked area chart. The same rows as a line; drawn only when named.
- `line`: a date or time first, then 1-3 numeric columns. Up to 199 rows.
- `bar`: a label or period first, then 1-3 numeric columns. The fallback.

`before_and_after`: did it change after a dated event?
- `interrupted`: the x axis, then one value. Give `split_at` (the date of the
  change) and `split_label` (the change in a few words). Keep enough rows on
  each side to show the usual range.
- `aligned`: what a measure does around a repeated event. event (when each
  occurrence began), offset (minutes from it, negative before) and the
  measure. Give `event_label`. See references/aligned.md.

`usual_range`: is this normal for me, and how much does it vary?
- `baseline`: the x axis, the value, usual_low and usual_high. The band is the
  owner's own usual, never a target. See references/baseline.md.
- `dotplot`: one value per record (a number, a duration or a time of day), an
  optional label first. 3 to 400 rows; don't aggregate them.

`compare_groups`: how do these groups compare, or then against now?
- `multiples`: small multiples. panel (the group's name), the x axis and the
  value. 2 to 12 panels, in row order.
- `dumbbell`: a category, then exactly two numbers named for the two moments.
  Up to 20 rows.
- `list`: the sorted bar list. A label, then a value, largest first, up to 20
  rows. Never use it to rank people.
- `bar`: the fallback.

`part_to_whole`: what is the total made of?
- `share`: the 100% bar. A label, then a value, 2-8 parts of one whole.
- `waterfall`: a label, then an amount that adds (positive) or takes away
  (negative). The first row may be the start.
- `stack`: stacked bars. The x axis, then 2-5 part columns, stacked from the
  bottom in column order.

`relationship`: does one measure move with another, or when in the week does it happen?
- `scatter`: a date or label, then two numbers; or one column of times for a
  weekly rhythm. 3 to 400 rows. Give `finding`, one sentence on what lines up,
  written only from rows you have already read.

`daily_rhythm`: when in the day do things usually happen, across many days?
- `states`: the state timeline. day, start, end and state, at most 6 states.
  Name the states as the reply does.
- `ridgeline`: day, then hour (0-24) or minute, then the value. Up to 100 days.
- `diurnal`: the diurnal profile. hour (0-23), days (weekdays or weekends) and
  share (0-1). Up to 48 rows.
- `actogram`: a daily window across many days, such as asleep to awake or first
  to last. day, started_at and ended_at.

`events`: what happened, item by item?
- `swimlanes`: lane (the group; thread or project work too), at and what. Up to
  12 lanes, in row order.
- `lollipop`: what is due ahead. due (a date), label, and an optional amount in
  dollars. `days` (7-92, 30 if left out) sets the window, which starts today.
  Leave out items whose date has passed.
- `itinerary`: stays in order. place, started_at and ended_at, and an optional
  mode (Train, Flight) only when a record says so. The first stay may have no
  started_at and the last no ended_at.
- `timeline`: start, an optional end, and label. The fallback.

`place`: where?
- `route`: one movement. lat, lon and at, in order. 2 to 199 points; thin a
  long trace.
- `density`: where time went. lat, lon, minutes and an optional label. Give
  `period` (in October). Places too far apart for one map belong in a `list`.
- `map`: lat, lon and an optional label. The fallback.

`in_sentence`: which one to four figures belong in the sentence?
- `numbers`: one row of 1-4 columns, each a figure. Add `series` when a figure
  stands for rows the owner may want to open. See references/numbers.md.
- `spark`: numbers with series, each drawn as a sparkline after its figure.

`table`: which rows should the owner read?
- `table`: up to 8 columns; 50 rows show.

## Once it draws

Say what the rows show without listing them again, and quote no figure that is
not in them.
</charts>
