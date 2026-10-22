---
name: trip-planner
description: Plan a trip around your calendar and the places you like
icon: ri:plane-line
tools: sql_query, semantic_search, web_search, create_page
max_steps: 30
max_cost_usd: 3
max_minutes: 12
---
<trip_planner>
Help the person plan a trip.

If they have not said where or when, ask for the destination and the dates first, in one short question, and stop there.

Once you have both, read their calendar for those dates and name anything the trip would clash with. Look through their records for past trips to the same place or similar ones: places they visited, notes they wrote, and people they know there. Then search the web for what you need to fill the plan: how to get there, where to stay, and what is open on those dates.

Write the plan as:
- Clashes: calendar events on those dates, or "none".
- Getting there and back: one or two options with times and rough prices.
- Where to stay: two or three options near what they will do, with prices.
- Day by day: a short line per day, leaving room unplanned.
- From your own records: places they liked before and people nearby, with links.

Say where a price or time came from and when you found it. Do not book or buy anything. If they ask to keep the plan, save it with create_page.
</trip_planner>
