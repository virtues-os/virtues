---
title: Glossary
description: The words Virtues uses for the parts of your life it holds — records, days, events, articles, pages, notebooks, applets — and what each one actually means.
updated: 2026-08-28
---

Virtues names things deliberately, and uses one word per thing. This is that
vocabulary, in roughly the order you meet it.

## The machine

**Server** (or **box**) — the machine in your home running Virtues OS. It
holds your data, does the thinking, and answers your devices. Nothing about it
depends on us being reachable.

**Device** — a phone or laptop paired with your server. Each one has its own
key, and the server keeps a list of which keys are allowed. That list is the
whole of authentication; there's no password to steal. See
[Reaching your server](/docs/operate/reach).

**Channel** — the release line your box follows, stable or prerelease. See
[Upgrading](/docs/operate/upgrading).

## What comes in

**Source** — somewhere your life is already recorded that the box can pull
from: a calendar, a mail account, a health app, a bank, your phone's location.
Connecting a source is what starts the flow.

**Record** — one thing a source observed. A message sent, a place visited, a
song played, a transaction, a heart rate sample, a recording. Records are the
raw material and the box keeps them as they were; everything else is derived
and can be rebuilt.

**Lake** — where the large things live: recordings, uploads, files. The
database holds the structure of your life, and the lake holds its bytes.

## What the box makes of it

Everything below is *derived* from your records. It can be recomputed, which
is why rebuilding an index or re-running a day is safe.

**Event** — a bounded stretch of a day. A meeting, a drive, a walk, a meal.
The box assembles events out of records that arrive incomplete, out of order,
and sometimes contradicting each other.

**Day** — the unit of narrative, and the one most worth reading. A day carries
its own timeline of events and a written account of what happened, generated
overnight from everything the box saw.

**Article** — a page about a subject rather than a span of time. The people,
places, and organizations in your record each get one, accumulating what's
known about them.

**Note** — a smaller observation attached to a subject: a correction, an
appraisal, something worth remembering about it.

**Ref** — a pointer from something the box wrote back to the record it came
from. Refs are what make a claim checkable rather than merely plausible; when
the box tells you something about your life, a ref is how you see why it
thinks so.

**Wiki** — where all of the above lands: the record's own encyclopedia. Days
and their events on one axis, people, places, and organizations on the other,
articles accumulating on both — and your chapters and narrative identity
giving the whole thing its coordinates.

## What you author about yourself

These are not derived and are never recomputed. The machine may write each one
only while it is empty; from then on it is yours.

**Narrative identity** — the account of who you are that grounds everything
the models see: your history, in your own words, arranged from the interview
and edited on its page from then on.

**Chapter** — your own division of your life into named eras, rough years and
all, given in the interview. Every day the record holds falls inside exactly
one chapter; each chapter has a page of its own.

## What you make

**Page** — a document you write, versioned as you go, and shareable.

**Notebook** — a working lens over your life: a place to gather material
around a question, with chat that's grounded in what you gathered rather than
in everything.

## What you can build

**Applet** — a small program running on your box on a schedule or on demand:
pulling from a source, computing something, keeping a record of its own. Some
ship with Virtues, and some are written on the box — including by asking for
them in chat. Each run is recorded, so an applet that quietly stops working is
visible rather than silent.

## Words we're careful with

**Observed, authored, generated** — the distinction between what a sensor
recorded, what a person wrote, and what a model produced. It matters most in
the last case: something the box generated is never treated as evidence on its
own, which is why generated text carries refs back to records.

**Derived** — anything the box computed and could compute again. Indexes,
events, summaries. The opposite of your records, which are the only things
that can't be regenerated and which backups exist to protect. See
[Backup & restore](/docs/operate/backup-and-restore).
