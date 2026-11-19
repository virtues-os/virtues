# The baseline band

Each row carries the value and the owner's own usual range for that point:
`usual_low` and `usual_high`. Compute the range from the rows before each
point, so the band moves as the owner's usual does:

    SELECT v.day,
           v.value AS "Resting heart rate",
           percentile_cont(0.1) WITHIN GROUP (ORDER BY v2.value) AS usual_low,
           percentile_cont(0.9) WITHIN GROUP (ORDER BY v2.value) AS usual_high
      FROM <daily values> v
      JOIN <daily values> v2 ON v2.day BETWEEN v.day - 28 AND v.day - 1
     GROUP BY v.day, v.value
     ORDER BY v.day

A 28-day window and the 10th to 90th percentile are a reasonable default.
Say in the reply what the band is. It is never a target.
