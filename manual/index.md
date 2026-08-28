---
title: What Virtues is
description: Virtues is a server that holds the data of your life under your own roof, and the software that turns it into a readable record. Start here.
updated: 2026-08-28
---

Virtues is a server that holds the data of your life — messages, calendar,
health, location, transactions, recordings, files — on a machine you own, and
the software that turns that pile into something readable: a record of your
days, articles about the people and places in them, and an AI that reasons
from your actual life rather than a generic profile.

It runs on your own Linux machine today. Purpose-built hardware comes later;
nothing on this site requires it.

## What runs where

This is the part most people want answered first, so here it is plainly.

**Your records stay on the box.** Sources are pulled in and written to a
database and a file store on your own disk. Nothing syncs them out. The
embedding and reranking models that make your record searchable also run on
the box, locally, on ports nothing outside it can reach.

**The language models that write do not run on the box.** When Virtues
composes an account of your day or answers a question, the material for that
request goes to a model provider — through our gateway by default, or through
any OpenAI-compatible endpoint you point it at, including a provider you have
your own account with. We meter what a request cost and never keep what was in
it.

**No inbound port is opened at home, and we have no way in.** Your devices
reach the box by key, on a list the box itself keeps. We aren't on that list.
[Reaching your server](/docs/operate/reach) describes the paths and states
exactly what our relay can and cannot see, without rounding it up.

## Where to start

If you're installing on your own hardware, [Installing](/docs/setup/install)
is one command and the ten minutes around it. After that,
[Reaching your server](/docs/operate/reach) covers pairing a phone or laptop,
which is how you actually use the thing.

Once it's running, four pages carry the weight:
[Upgrading](/docs/operate/upgrading),
[Backup & restore](/docs/operate/backup-and-restore) — read that one before you
need it, since the box deliberately cannot decrypt its own archives —
[When something breaks](/docs/operate/recovery), and
[The CLI](/docs/operate/cli).

The [Glossary](/docs/understand/glossary) defines the words this system uses
for the parts of your life it holds, which is worth ten minutes if the
vocabulary feels invented. It partly is.

## What's here, and what isn't

These docs are written alongside the software and describe what actually
ships. Pages marked *soon* in the sidebar are planned; they land as the
features they cover settle, rather than in advance of them. Nothing published
here is aspirational — if a page says the box does something, it does.

Alongside the manual are the [engineering notes](/docs/notes): the design
records, audits and measured findings from building this. They are working
documents rather than documentation, each carrying a status, and they are
public because the reasoning behind a decision is usually worth more than a
summary of it. Read them for *why*; read these pages for *how*.

The [Library](/library) holds the essays — what we think this is for. That's
the other register entirely.
