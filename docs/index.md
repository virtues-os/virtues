---
title: What Virtues is
description: Virtues is a server that holds the data of your life under your own roof, and the software that turns it into a readable record.
updated: 2026-09-15
---

Virtues is a server you run at home. It pulls in what you already generate,
keeps it in one database on your own disk, and turns it into a record you can
read. It runs on your own Linux machine today, or on a server we build.

Two ways in. If we built your server, pair a device:
[Reaching your server](/docs/operate/reach). If it is your own machine, read
[what to run it on](/docs/setup/requirements), set up
[inference](/docs/inference), then run the [installer](/docs/setup/install).
The two paths meet at pairing and share everything after it.

## What it does

**It writes your days.** After a day ends, the server rebuilds it from
evidence, visits, movement, messages, purchases, sleep, into a timeline, then
writes the account. A plan with no trace behind it becomes an honest _unknown_,
not a confident memory.

**It keeps a wiki of the people and places in them.** The Sarah in your
calendar, contacts, and messages resolves to one person with a page of her own.

**It answers from your data.** The assistant has read-only SQL over your
tables, search over the record, and a sandbox. It answers from what happened,
not from a profile.

Each day is written after it ends, so the first page worth reading arrives
tomorrow. `virtues status` shows what is flowing in the meantime.

## What leaves the server

Your record stays on your disk, and so do the two models that make it
searchable: the installer refuses to point them at anything but a local
address. Two things go out. The model that writes your days and answers your
questions receives the relevant part of your record for each request, through
our gateway, which meters the cost, keeps nothing, and routes only to provider
endpoints under a zero-retention agreement. A voice recording goes out as
audio, since transcription happens at the model. Point the writing slot at a
model on your own hardware and both stop.

Nothing comes in. The server opens no inbound port, your devices reach it by
key on a list only the server keeps, and we are not on that list. The relay
carries sealed bytes it cannot read. [Reaching your server](/docs/operate/reach)
shows the paths. The
[privacy model](https://github.com/virtues-os/virtues/blob/main/agents/record/privacy-model.md)
is the full account, including exactly what each kind of request sends.

## How to read this manual

These pages are written alongside the software and describe what ships. Pages
marked _soon_ in the sidebar are planned. If a page says the server does
something, it does.

Every page is plain markdown: append `.md` to any docs URL for the source, and
[`/llms.txt`](/llms.txt) indexes them all for anything reading on your behalf.
The manual publishes from the released branch, so what you read is what a
server actually runs.

The engineering record, design decisions, audits, and measured findings, lives
in the [repository](https://github.com/virtues-os/virtues). The
[Library](/library) holds the essays. Neither is this manual.
