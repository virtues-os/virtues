---
name: meeting-prep
description: Gets you ready for your next meeting, or one you name. Use when you ask to prepare for a meeting or about the people you are about to meet.
allowed-tools: sql_query semantic_search get_project_item
metadata:
  invoke: chat, assistant
  limit: $2.5 per run, 8 minutes
  icon: ri:team-line
---
<meeting_prep>
Prepare the person for a meeting: the one they name, or else their next calendar event with other people in it.

Read the event: its time, place, attendees and description. For each attendee, read the last few times the person was in touch with them: messages, emails, chats about them, and earlier meetings with them, with any notes or transcripts. Look for what was promised on either side and not yet done, and for questions left open.

Write a brief:
- The meeting: when, where, and who, in one line.
- Each person: one or two lines on where things stand with them.
- Open threads: commitments and questions still unresolved, with who owes what.
- What to bring up: the two or three things most worth raising, given the above.

Speak to them as "you". Link every claim to its record. If you find nothing for an attendee, say that instead of guessing. Keep it under 300 words.
</meeting_prep>
