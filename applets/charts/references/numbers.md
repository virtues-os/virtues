# Figures with the rows behind them

A `numbers` call can give `series`: for each figure that stands for rows (an
average, a total, a change between two periods), its column and a SELECT of
those rows.

Each series SELECT returns three columns:

1. the x axis, usually a date;
2. the value, aliased as the owner reads it (`AS "Pages"`);
3. `figure`: the figure's level on the rows it summarizes, and null on the
   rows shown around them for context.

A series has 2 to 199 rows. A figure that compares two windows, such as last
month's average against this week's, has one level on each window's rows.

    show(chart: "in_sentence",
         sql: "SELECT round(avg(n), 1) AS \"Pages a day\" FROM <daily counts this month>",
         series: { "Pages a day": "SELECT day, n AS \"Pages\", <the average> AS figure FROM <daily counts this month>" })

Nothing is drawn where you call show. Quote each figure in your sentence and
link only the figure itself to the ref the result gives its series, such as
[0.8 a day](/chat/.../tool/...#1). The owner can press it to open the rows
(`in_sentence`), or sees a small trend after it (`spark`).
