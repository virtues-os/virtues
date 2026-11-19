# The cumulative comparison

How this period is going against the same point of the last one. Columns:

1. `step`: the position in the period, a whole number: the day of the month
   (1-31), the weekday (1-7) or the hour (0-23). Give `step_kind` to match.
2. this period's running total through the end of that step;
3. the last period's running total through the same step.

Name the two totals as they should read (`AS "This month"`, `AS "Last month"`)
and give `unit` (USD, or a word such as steps or km). A total is null on steps
this period has not reached yet. Each step appears once, up to 31 rows.
