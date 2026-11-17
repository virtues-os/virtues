---
name: clarity-review
description: Rereads three of today's conversations with AI and shows where you could have said what you meant more plainly. Use when you ask how you could have worded or asked things better today.
allowed-tools: sql_query create_page
metadata:
  invoke: chat, assistant
  limit: $1 per run, 5 minutes
  icon: ri:feedback-line
---
<clarity_review>
Reread three of the person's conversations with AI from today, and show them where their own words could have been clearer.

First, list today's conversations. Run this query as written:

  SELECT 'virtues' AS app, c.id AS conversation_id, c.title,
         count(*) AS their_messages,
         (array_agg(left(m.content, 80) ORDER BY m.created_at))[1] AS opening
    FROM app_chats c
    JOIN app_chat_messages m ON m.chat_id = c.id
   WHERE c.deleted_at IS NULL
     AND m.role = 'user'
     AND m.created_at >= current_date
     AND coalesce(m.subject, '') <> 'onboarding_synthetic'
     AND m.content !~ '^/[a-z0-9-]+(\s|$)'
   GROUP BY c.id, c.title
  UNION ALL
  SELECT provider, conversation_id, NULL,
         count(*),
         (array_agg(left(content, 80) ORDER BY occurred_at))[1]
    FROM data_content_conversation
   WHERE role = 'user'
     AND occurred_at >= current_date
     AND deleted_at_source IS NULL
   GROUP BY provider, conversation_id
   ORDER BY their_messages DESC
   LIMIT 6

Leave out the conversation this request came from. Take the first three rows that remain, which are the conversations where they wrote the most.

Read each one with the query for its app. The person's messages come back in full and the replies are cut short.

  When app is virtues:
  SELECT role, CASE WHEN role = 'user' THEN content ELSE left(content, 600) END AS content
    FROM app_chat_messages
   WHERE chat_id = '<conversation_id>' AND role IN ('user', 'assistant')
     AND created_at >= current_date
   ORDER BY sequence_num

  For any other app:
  SELECT id, role, CASE WHEN role = 'user' THEN content ELSE left(content, 600) END AS content
    FROM data_content_conversation
   WHERE conversation_id = '<conversation_id>' AND occurred_at >= current_date
   ORDER BY occurred_at

Review only the person's messages. The replies show how each message was understood. Look first for these signs that a message did not say what they meant:
- they restated or corrected what they had asked;
- the reply asked what they meant, or answered a different question;
- one message asked two things and only one was answered;
- a word such as "this" or "it" left the reply to guess what they meant;
- they rejected an answer over a requirement they had never stated.

Also look for one concrete question they approached in two or more messages without asking it directly.

Then write the reply. It begins at the first heading. For each conversation, a short heading with its title, or its opening words when it has no title, and the app it was in, named as people write it: Virtues, ChatGPT, Claude, Gemini or Grok. Under each heading, one of two shapes:

  If their messages were clear and got them what they asked for, one sentence saying so.

  Otherwise, one to three moments, strongest evidence first. Each moment has their words, quoted exactly and briefly; one sentence on what made them unclear; and a rewrite in their own voice that says the same thing plainly. If they approached a question without asking it, end with "The question underneath:" and that question, worded as they could have asked it.

The reply ends with the last conversation's section. Only when the same habit shows in two or more conversations does one more sentence follow it, naming that habit.

If the first query returned no rows, the whole reply is one sentence: you found no conversations with AI from today, and conversations in other AI apps reach the box only once they are imported or synced. If it returned one or two, the reply has that many sections.

Speak to them as "you". Keep to the wording: say nothing about their motives, feelings or character, and do not grade or score anything. A rewrite keeps their meaning and their tone. Keep the whole reply under 450 words.

If they ask to keep it, save it with create_page and title it with today's date.
</clarity_review>
