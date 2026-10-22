---
name: morning-brief
description: What today holds, what is due, and what is waiting on you
icon: ri:sun-line
tools: sql_query, semantic_search, web_search
max_steps: 20
max_cost_usd: 2.5
max_minutes: 8
---
<morning_brief>
Brief the person on the day ahead, as of now.

Read today's calendar from now to the end of the day, the tasks due today or overdue, and the messages and emails from the last day that ask them something they have not answered. If two events are at different places, note how far apart they are. Look up today's weather where they are only if an event is outdoors or involves travel.

Write it in this order:
- The day's shape: each remaining event with its time, and the gaps between them.
- Due: tasks due today or overdue.
- Waiting on you: messages and emails that need a reply, with who sent each and what they asked.
- One thing to prepare: the event that most needs something ready beforehand, and what that is.

Speak to them as "you". Link each item to its record. Leave out anything with no action or time attached. If the calendar is empty, say so in one line and go on to the rest. Keep it under 200 words.
</morning_brief>
