# About this chat: what a chat named, cited, made and sent out

**Status: slice 1 built on 2026-10-29.** The chat's context page
(`/chat/<id>?view=context`, `chat/ContextViewPanel.svelte`) is now the chat's
About page. Its body is `chat/about/InThisChat.svelte`, and the transcript is
read in `chat/about/inThisChat.ts`. Slices 2 to 5 are not built.

Decided 2026-10-29: no panel. A per-pane panel docked on the right edge was
built on 2026-10-28 and removed the next day. It read as a second sidebar
inside the page, an IDE inspector, and the window has no rail (design.md, The
shell). What a chat touched is a page of its own, opened beside the chat.

## What it is

One page per chat, opened beside it. From the top:

1. **Head:** the chat's title and when it started.
2. **Mentioned:** the people, places, pages and files the chat is about, from
   either side: what you attached or @-mentioned, then what the replies named.
3. **Sources:** the records and web pages the replies cite, with "Also read,
   not cited" folded under them. That fold leaves out anything listed in
   another group.
4. **Made here:** maps, charts, tables, new and edited pages, applets, tasks.
5. **Left your server:** the model that wrote the answers, with its provider,
   then every web search, browse and image model call. A sentence names the
   providers the answers went to. It is always drawn.
6. **How it ran,** folded: messages, model, tokens, cost, what the next message
   carries, and Compact. This was the whole context page before.

The page lists only what the transcript carries. It never runs a model pass of
its own, so it cannot name something the chat did not touch, and a chat opened
next week rebuilds the same page.

## Who it is for

- **Narrative seeker.** The chat keeps its full width; nothing sits beside it
  unless you open this page.
- **Quantified self.** Every chart, map and table the chat made is in one list,
  and each one scrolls back to its place in the chat.
- **Tech prosumer.** Rows and the chat's links light each other across the two
  panes. The token detail is kept under How it ran.
- **Recovery.** No counts above the fold. Lists end in "Show all", never a
  number. The counts the context page always had stay folded in How it ran.
- **Data sovereign.** Left your server names the model's provider. Every chat
  turn sends the question and the records it read there unless the slot points
  at a local model (privacy-boundary.md).

## Interaction

- **Opening it:** the info button in the chat's pane strip (held down while the
  page is open), ⌘⌥B, or About this chat in the chat menu. It opens in the other
  pane, splitting the window if needed. The same button or ⌘⌥B closes it.
- **Rows** open in a new tab beside the About page, so the chat in the other
  pane stays put. A web page opens outside the app.
- **Show in the chat** steps through the messages a row appears in. The page
  sends `REVEAL_MESSAGE` and the chat view showing that chat scrolls and tints
  the message. When no pane shows the chat, the page opens it beside first.
- **Echo.** Hovering a row tints its links in the chat (`.ref-echo`, matched by
  `data-ref-url` on `Ref`). Hovering a link in the chat tints its row.
- **Groups** fold from their label like the sidebar's, and the fold is
  remembered (`virtues.aboutChat.folds`). Also read and How it ran start folded.
- **The tab** is labelled "About this chat". `identityOf` keeps the opened label
  for a `?view=` route, so the tab does not take the chat's title.

A page appears once under one key. A reply cites a page as a record
(`/record/app_page/<id>`), while a mention and `get_page_content` name it as
`/page/<id>`. So `inThisChat.ts` keys and opens every page as `/page/<id>`, and
it takes the title from the read when the link text is only a word like
"page".

## Slices

1. **Built.** The About page as above.
2. **Cards and maps.** A record row opens its `RefCard` instead of a tab; map
   pin emphasis; tinting the message for rows that have no link (a chart, a
   search).
3. **One answer.** Clicking an answer's status label ("Worked for 9s …",
   `ThinkingBlock.svelte`) opens the page filtered to that turn.
4. **Local models.** When a slot points at a local model, Left your server says
   the answers stayed on the server. The web app has no flag for that yet.
5. **Phone.** The page as a sheet over the chat.

## Open questions

- Whether a page gets the same treatment ("About this page": mentioned, linked
  from, edits), replacing the backlinks in `PageContent.svelte`.
- Whether Made here lists page edits a chat made, or only what it created.
- Provider names come from the stored `provider` string with its first letter
  capitalized ("Deepseek"). The gateway's own display names would be better.
