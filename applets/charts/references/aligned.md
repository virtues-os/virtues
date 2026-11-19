# The event-aligned overlay

What a measure usually does before and after something that repeats: payday,
a workout, arriving somewhere. Columns:

1. `event`: when each occurrence began, a timestamp or a date;
2. `offset`: minutes from that start, negative before (1,440 is a day);
3. the measure, aliased as the owner reads it.

Bin the offsets in the query: to 5 minutes across a few hours, or to a day
across a few weeks. Up to 1,200 rows and 40 occurrences. Give `event_label`,
the event in a few words, such as Payday.
