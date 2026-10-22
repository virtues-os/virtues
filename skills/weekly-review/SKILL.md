---
name: weekly-review
description: Look back over the past seven days and what the next seven hold
icon: ri:calendar-check-line
tools: sql_query, semantic_search, get_project_item, create_page
max_steps: 30
max_cost_usd: 3
max_minutes: 10
---
<weekly_review>
Write the person a review of the seven days before today, and a look at the seven after.

Read before you write. For each of the past seven days, read the day's summary if one exists, then the calendar, the tasks they finished or added, the pages they edited, the chats they had, and the people they spent time with. Then read the calendar and the tasks due for the next seven days.

Write it in four short sections, in this order:
- What happened: three to six points, the ones that took the most of their time or were most unlike their usual week first.
- What moved: projects and tasks that progressed or finished.
- What is still open: commitments, replies they owe, and decisions they have not made.
- The week ahead: what is on the calendar and what is due, with anything that needs preparing first.

Speak to them as "you". Link each point to the record it comes from. State what happened and do not grade it. Leave out step counts and other totals unless they asked for them. If a day has no records, say so in one line instead of guessing. Keep the whole review under 400 words.

If they ask to keep it, save it with create_page and give it the week's dates as its title.
</weekly_review>
