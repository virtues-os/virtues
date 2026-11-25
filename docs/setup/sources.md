---
title: How sources work
description: Connect the accounts and devices your Virtues server reads from - iPhone, Mac, Google, Outlook, Notion, Strava, banks, meeting notes from Fathom, Fireflies.ai and Granola, GitHub stars, X bookmarks, Instagram saves, past chats, and downloads from Spotify, Amazon, Discord and Slack - and the tools your assistant can use, from the Sources page.
updated: 2026-11-25
---

A source is something your server reads from: a device, an account, or an
export you bring in once. Your server keeps what each one sends, and your
assistant answers from it.

1. Open **Settings**, then **Sources** on the **Server** line.
2. Under **Add a source**, choose **Connect** beside the one you want. A
   device shows **Pair**, and past chats show **Import**.
3. For an account, the window says what your server will read and what your
   assistant may do there. Choose **Continue** and sign in on the service's
   own page.
4. Come back to Virtues. The window says the account is connected, and the
   source moves to **Connected**.

Open any source to see what your server reads from it, how its last runs
went, and the accounts or devices connected to it.

## Next steps

Pair your phone with [Connecting your devices](devices.md) if you haven't
yet. Your iPhone sends more than any account.

---

## What each source brings

- **iPhone:** health, places, audio, calendar, contacts and photo favorites,
  sent by the Virtues iPhone app. You allow each kind in the app on the
  phone. For each photo or video you favorite in Photos, your server keeps
  when and where it was taken and a smaller copy of the picture (one frame,
  for a video). A model provider describes each picture so you can search
  for what is in it, under Photo favorites in Storage. iOS asks for your
  whole library, but the app reads only favorites, never your Hidden album.
  The originals stay on your phone. Unfavorite, hide or delete a photo and
  its picture, place and description leave your server; a note you wrote
  stays.
- **Mac:** messages, the apps you use and your browsing, collected by the
  Virtues desktop app. What it may read depends on the permissions you grant
  in System Settings on that Mac.
- **Google:** your mail and calendar.
- **Outlook:** your mail and calendar, from an Outlook.com or Microsoft 365
  account. Your server reads them and never changes them.
- **Notion:** your pages and databases.
- **Strava:** your runs, rides and swims.
- **Banks:** spending, balances and investments, through Plaid.
- **Fathom, Fireflies.ai and Granola:** your meetings, each with its
  transcript, summary and attendees, and action items where the tool keeps
  them. You paste a key you make in the tool; its connect window says where.
  Your server reads from the tool directly with that key. Day pages place
  each meeting at its time beside your recordings. Fathom also asks for your
  email in Fathom, because its key can see meetings shared with your team,
  and your server keeps only the ones you recorded or were invited to.
  Granola makes keys on its Business and Enterprise plans.
- **GitHub:** the repositories you star, with a read-only access token you
  create on GitHub.
- **X:** the posts you bookmark. In the Mac app you sign in to x.com in a
  window. In a browser you paste the two cookies that hold your sign-in.
- **Instagram:** the posts you save. In the Mac app you sign in to Instagram
  in a window. In a browser you paste its cookies.
- **Past chats:** your conversations from a Claude or ChatGPT export. A newer
  export adds only what is new.
- **Spotify, Amazon and Discord:** a data download you request from each
  service and import here, the same way as past chats. Spotify's extended
  streaming history brings every song, podcast and audiobook you played.
  Amazon's download brings each item you ordered, without your addresses or
  payment details. Discord's brings the messages you sent; it has no one
  else's. The import window says where to request each file. A newer
  download adds only what is new.
- **Spotify's recent plays:** between downloads, your server can add the
  songs you play from an app you make on Spotify's developer site. Open
  Spotify in Sources and choose **Connect your Spotify app**; the window
  lists the steps. Spotify asks the app's owner to have Premium and asks you
  to sign in again every six months, and this way does not include
  podcasts. When you sign in, Spotify sends your browser through
  `auth.virtues.com` on its way back to your server, carrying a one-time
  code that works only with a secret your server keeps. Your server then
  reads from Spotify directly. A later download replaces these plays with
  its fuller record where the two match.
- **Slack:** a workspace export, which only the workspace's admins can make.
  The import window asks for your email in Slack, and your server keeps only
  the conversations you are in. On Slack's free plan an export reaches back
  90 days.

## Where Google and Outlook mail travels

Google and Microsoft each ask an app that reads mail to pass their own
review, and Virtues has not finished either yet. Until it has, an Outlook
account, and a Google account on a server linked to a Virtues account, are
signed in through a sign-in service that has passed them. Each request and
answer for that account goes from your server through `api.virtues.com` and
that service to Google or Microsoft, and back. Neither stores your mail or
events, and the service keeps the sign-in. Outlook is read only: your server
never changes your Outlook mail or calendar.

## Tools your assistant can use

Todoist and Linear are listed under **Tools your assistant can use**, and
**Add it by its address** connects another service that publishes an MCP
server you can sign in to. Your server does not sync these. Your assistant works in them when
you ask, and what it reads there is kept in that chat's history on your
server.

## What your assistant can do in Google

Google's page has two switches under **What your assistant can do**:

- **Archive, label and draft mail.** It never sends mail. A draft waits in
  Gmail until you send it, and every change can be undone.
- **Add and change events.** It asks you before it deletes one.

Turning a switch off stops your assistant using those tools in a chat and
in sites that run on their own. The same switches are on your assistant's
page in Settings, **What … can do**, with its other tools.

## When a source needs you

When a sign-in expires or a device loses a permission, the Sources page
says so in a sentence at the top, with the button that fixes it. A sign-in
that runs out keeps what your server already has, and signing in again
picks up from there.

If your subscription ends, your server lets your banks go after 30 days,
because Plaid charges for each linked bank. Sources says so when it
happens, and you can connect them again at any time.

## Devices

The iPhone and Mac pages in Sources show each paired device: when it was
paired, when your server last heard from it, the version of its app, and
**Unpair**. A Mac's page also lists what it may read. The iPhone's page
lists the places where it records no audio. You set those places from a
place's page, and the nightly schedule in the Virtues app on the phone.

**Settings › This device** holds the settings for the device you are using.

## Disconnecting

**Disconnect** on a source's page stops your server reading that account,
and anything that runs from it stops. What your server already has from it
stays. **Unpair** does the same for a device.

## Streams and Query

Every stream your server keeps, and how much each holds, is in **Settings ›
Your data › Streams**. **Query** on the same page reads any table with SQL.
