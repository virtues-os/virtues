---
title: How sources work
description: Connect the accounts and devices your Virtues server reads from - iPhone, Mac, Google, Notion, Strava, banks, GitHub stars, X bookmarks, Instagram saves and past chats - and the tools your assistant can use, from the Sources page.
updated: 2026-11-21
---

A source is something your server reads from: a device, an account, or an
export you bring in once. Your server keeps what each one sends, and your
assistant answers from it.

1. Open **Sources**, the button in the top right corner of the window, beside
   Search and Settings.
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

- **iPhone:** health, places, audio, calendar and contacts, sent by the
  Virtues iPhone app. You allow each kind in the app on the phone.
- **Mac:** messages, the apps you use and your browsing, collected by the
  Virtues desktop app. What it may read depends on the permissions you grant
  in System Settings on that Mac.
- **Google:** your mail and calendar.
- **Notion:** your pages and databases.
- **Strava:** your runs, rides and swims.
- **Banks:** spending, balances and investments, through Plaid.
- **GitHub:** the repositories you star, with a read-only access token you
  create on GitHub.
- **X:** the posts you bookmark. In the Mac app you sign in to x.com in a
  window. In a browser you paste the two cookies that hold your sign-in.
- **Instagram:** the posts you save. In the Mac app you sign in to Instagram
  in a window. In a browser you paste its cookies.
- **Past chats:** your conversations from a Claude or ChatGPT export. A newer
  export adds only what is new.

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
