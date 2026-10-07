# Bookmarks — what is still open

The design and everything built is recorded in
[`../record/bookmarks.md`](../record/bookmarks.md). Each item below was checked
against the code on 2026-09-29.

## 1. Wire the iOS share extension

The extension (`apps/web/src-tauri/gen/apple/ShareExtension/`), the app-side
drain (`ShareInbox.swift`) and the box arm (`ios_ingest/bookmark.rs`) are
built. The extension is in no Xcode target. In order, each step depending on
the one before:

1. ~~Register the App Group and `com.virtues.app.share` with it.~~ Done
   2026-10-07, for the Control Center control (audio-schedule-places-plan.md
   slice 5), which needed the same group.
2. ~~Add the application-groups entitlement to the app.~~ Done the same day.
   `ShareInbox` now finds the shared container; until the extension writes
   to it, there is nothing to drain.
3. Add the target by editing the project in place — **never by running
   `xcodegen generate`**. Measured 2026-09-23: regenerating drops five
   Tauri-merged `Info.plist` keys (Bluetooth — BLE onboarding crashes without
   it — HealthUpdate, camera, photo library, icons) and rolls the version back.
   Review a diff containing the extension and nothing else; mirror it into
   `project.yml` only so the spec stays truthful.
4. Stamp the extension's version from the app's at build time. As written it
   reads `$(MARKETING_VERSION)`, which the project does not define, and it must
   equal the app's or upload fails.
5. Release the box first. An app with the extension talking to a box without
   `externalize_images` loses screenshot-only shares while reporting success.
6. On a real phone, measure how long a shared link and a screenshot take to
   reach the box. That number decides whether any wake-the-app workaround is
   warranted.

## 2. `data_content_post`

Own posts are authored content, not saves, and fit neither
`data_content_conversation` (role is constrained to AI-chat values) nor
`data_communication_message` (a public post is addressed to no one). Proposed:
`post_type` (post | reply | repost | quote), `text`, `url`, `conversation_id`,
`in_reply_to_id`, engagement in `metadata`. Settle the shape before the
migration; every future social source lands in it.

## 3. Smaller items

- **Video and audio enrichment.** The duration tiers in the record are decided;
  nothing claims video or audio rows yet.
- **Colour search.** Needs dominant colours stored as hex per bookmark
  (deterministic, no model). The image pass writes `style` prose only.
- **Enrichment budget in Settings.** The daily cap is
  `VIRTUES_BOOKMARK_ENRICH_DAILY_CAP` only; no Settings knob, no priced
  "enrich all" button.

## 4. Browser extension auth

Webhook auth accepts only a paired device's key, which a browser extension does
not hold. Likely path: hand off to the Mac collector over native messaging
rather than give the extension its own identity — the same "hand off, don't
dial" answer as the share sheet.
