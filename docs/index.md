---
title: Overview
description: Virtues is a server you run at home that stores the data of your life on your own disk, and the software that turns it into a readable record.
updated: 2026-10-20
---

Virtues is a server you run at home. It pulls in data you already generate,
keeps it in one database on your own disk, and turns it into a record you can
read. It runs on your own Linux machine today, or on a server we build.

There are two ways to start. If we built your server, pair a device:
[Reaching your server](/docs/operate/reach). If it is your own machine, read
[what to run it on](/docs/setup/requirements), set up
[inference](/docs/inference), then run the [installer](/docs/setup/install).
Both paths reach pairing, and everything after pairing is the same.

## What it does

**It writes up your days.** After a day ends, the server rebuilds it from
evidence (visits, movement, messages, purchases, sleep) into a timeline, then
writes an account of it. A plan with no evidence behind it is marked
_unknown_ instead of being written up as something that happened.

**It keeps a wiki of the people and places in them.** The Sarah in your
calendar, contacts and messages resolves to one person with her own page.

**It answers questions from your data.** The assistant has read-only SQL over
your tables, search over the record, and a sandbox. Its answers come from what
is in your record.

Each day is written after it ends, so the first page worth reading arrives
tomorrow. `virtues status` shows what is coming in until then.

## What leaves the server

Your record stays on your disk, and so does the model that makes it
searchable: the installer refuses to point that model at anything but a local
address. Two things go out. The model that writes your days and answers your
questions receives the relevant part of your record for each request. That
traffic goes through our gateway, which meters the cost, keeps nothing, and
routes only to provider endpoints under a zero-retention agreement. A voice
recording goes out as audio, because the model does the transcription. If you
point the writing slot at a model on your own hardware, neither of these
leaves your network.

Nothing comes in. The server opens no inbound port. Your devices reach it by
key, the server keeps the list of allowed keys, and we are not on that list.
The relay forwards encrypted bytes it cannot read.
[Reaching your server](/docs/operate/reach) describes the connection paths. The
[privacy boundary](https://github.com/virtues-os/virtues/blob/main/agents/build/privacy-boundary.md)
covers everything in detail, including what each kind of request sends.

## How to read this manual

These pages are written alongside the software and describe what ships. Pages
marked _soon_ in the sidebar are planned and not written yet.

Every page is plain markdown: append `.md` to any docs URL for the source, and
[`/llms.txt`](/llms.txt) indexes them all for any tool reading on your behalf.
The manual publishes from the release branch, so it describes the version
servers actually run.

Design decisions, audits and measured findings are in the
[repository](https://github.com/virtues-os/virtues). Essays are in the
[Library](/library). Neither is part of this manual.
