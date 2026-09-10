---
title: What Virtues is
description:
  Virtues is a server that holds the data of your life under your own roof, and
  the software that turns it into a readable record. Start here.
updated: 2026-08-28
---

Virtues is a server that holds the data of your life — messages, calendar,
health, location, transactions, recordings, files — on a machine you own, and
the software that turns that pile into something readable: a record of your
days, articles about the people and places in them, and an AI that reasons from
your actual life rather than a generic profile.

You can run Virtues on your own Linux machine today. Purpose-built hardware is
optional and coming soon.

## Where data lives vs. where it is processed

**Your data lives on your hardware and reasoning happens in the cloud.** Virtues
stores your database, records, embeddings, and reranking models locally on your
disk. However, local hardware rarely has the capacity to run frontier reasoning
models.

When Virtues performs external reasoning requests (such as composing an account
of your day, transcribing audio, or answering user queries), the necessary
context passes through our gateway to curated model providers via explicit
zero-data-retention pipelines. We meter what a request costs for billing, but we
never keep what was in it.

**You can bring your own API keys.** If you prefer, Virtues can route requests
through any OpenAI-compatible endpoint or personal provider account instead. In
that scenario, request payloads bypass our gateway entirely, and data retention
is governed by your direct terms with that provider.

**Virtues has no administrative access to your server.** Devices authenticate
against an authorization list maintained entirely by your local instance. We
aren't on that list. We maintain no backdoors and open no inbound ports.
[Reaching your server](/docs/operate/reach) details the exact network paths and
specifies what our relay can and cannot see.

## Where to start

**If we built your hardware,** inference and install are already done. Your next
step is to pair your phone or laptop:
[Reaching your server](/docs/operate/reach).

**If you're setting up your own Linux machine,** three steps:

1. Check [what to run it on](/docs/setup/requirements): minimum specs and a
   supported OS.
2. Set up [inference](/docs/inference) the embedding and reranking models that
   search your record. Do it before you install, because the installer asks for
   their URLs.
3. Run the [installer](/docs/setup/install): one command, about ten minutes.

Then pair your phone or laptop: [Reaching your server](/docs/operate/reach).

Once you're up and running, four pages cover day-to-day operation:
[Upgrading](/docs/operate/upgrading),
[Backup & restore](/docs/operate/backup-and-restore) (read it before you need
it, since the server cannot decrypt its own archives),
[When something breaks](/docs/operate/recovery), and
[The CLI](/docs/operate/cli).

The [Glossary](/docs/understand/glossary) defines the words this system uses,
which is worth ten minutes if the vocabulary feels invented. It partly is.

## What's here, and what isn't

These docs are written alongside the software and describe what actually ships.
Pages marked _soon_ in the sidebar are planned. Nothing published here is
aspirational — if a page says the server does something, it does.

Every page here is also plain markdown: append `.md` to any docs URL to get the
source, and [`/llms.txt`](/llms.txt) indexes them all for anything reading on
your behalf. The manual is versioned with the software it describes — this site
publishes from the released branch, so what you read is what a server actually
runs.

The engineering record behind all of this — design decisions, audits, measured
findings — lives in the [repository](https://github.com/virtues-os/virtues)
rather than here. It is written for the people and agents building Virtues, and
it reads that way.

The [Library](/library) holds the essays — what we think this is for. That's the
other register entirely.
