---
title: Glossary
description: The words Virtues uses for the parts of your life it stores - records, days, events, articles, things, projects, tasks, pages, applets - and what each one means.
updated: 2026-11-16
---

Virtues uses one word for each thing. This page defines those words, roughly
in the order you'll meet them.

## The machine

**Server** - the machine in your home running Virtues OS. It stores your data,
runs the processing, and answers your devices. It keeps working when our
services are unreachable.

**Device** - a phone or laptop paired with your server. Each one has its own
key, and the server keeps a list of which keys are allowed. That list is the
only authentication, so there's no password to steal. See
[Reaching your server](/docs/operate/reach).

**Channel** - the release line your server follows, stable or prerelease. See
[Upgrading](/docs/operate/upgrading).

## What comes in

**Source** - a place where your life is already recorded that the server can
pull from: a calendar, a mail account, a health app, a bank, your phone's
location. Data starts coming in when you connect a source.

**Record** - one thing a source observed: a message sent, a place visited, a
song played, a transaction, a heart rate sample, a recording. Records are the
raw material, and the server keeps them unchanged. Everything else is derived
from them and can be rebuilt.

**Lake** - where large files live: recordings, uploads, files. The database
stores the structured data, and the lake stores the files.

## What the server makes of it

Everything in this section is *derived* from your records. It can be
recomputed, which is why rebuilding an index or re-running a day is safe.

**Event** - a bounded stretch of a day: a meeting, a drive, a walk, a meal.
The server assembles events from records that arrive incomplete, out of order,
and sometimes contradicting each other.

**Day** - the main unit you read. A day has its own timeline of events and a
written account of what happened, generated overnight from everything the
server collected.

**Article** - the page the wiki writes about one subject: a day, a year, a
chapter, a person, a place, an organization or a thing. Your server writes each
day's article overnight. For a person, place, organization or year, choose
Write the article on its page. If you turn on the Wiki Editor applet, your
server also writes the articles for your chapters and things, and keeps every
article current as new records arrive. Any sentence you write stays as you
wrote it.

**Note** - a smaller observation attached to a subject: a correction, an
assessment, something worth remembering about it.

**Ref** - a link from something the server wrote back to the record it came
from. Refs let you check a claim: when the server tells you something about
your life, the ref shows the record it based that on.

**Wiki** - your encyclopedia, where all of your information is collected. It
is organized by days and their events in one direction, and by people, places,
organizations and things in the other, with articles on both. Your chapters and
narrative identity provide the overall structure.

## What you write about yourself

These are not derived and are never recomputed. The server can write each one
only while it is empty. After that, only you edit it.

**Narrative identity** - your account of who you are, which every model sees as
context: your history, in your own words. It starts from the interview, and
you edit it on its page after that.

**Chapter** - a named era of your life, with rough years, that you give in the
interview. Every day in your record falls inside exactly one chapter, and each
chapter has its own page.

## What you make

One question sorts the first three: can it be finished? If it never can, it is
a thing. If it can, in more than one step, it is a project. If one step
finishes it, it is a task.

**Thing** - something in your life that goes on and can't be finished, and
that isn't a person, a place or an organization: a garden, a bike, guitar. You
name it, and it gets a page in your wiki. That page lists the projects and
tasks you put under it.

**Project** - something unresolved in your life that can be finished and takes
more than one step. You make it, and it ends when you finish it or drop it. A
project keeps its chats, pages and files together, and chat inside it answers
from what you collected instead of from your whole record. It can sit under a
thing. Projects can't contain other projects.

**Task** - one step. A task sits under a project, under a thing, or on its
own. A task can recur: when you finish it, the next one is due that long after
the day you finished, so one is open at a time. A task you hand to your
assistant is a request, which it works in the background.

**Page** - a document you write. It keeps versions as you go and can be
shared.

## What you can build

**Applet** - a small program that runs on your server on a schedule or on
demand. It can pull from a source, compute something, or keep a record of its
own. Some ship with Virtues, and some are written on the server, including by
asking for one in chat. Every run is recorded, so you can see when an applet
stops working.

## Terms with specific meanings

**Observed, authored, generated** - the difference between what a sensor
recorded, what a person wrote, and what a model produced. The server never
treats generated text as evidence on its own, which is why generated text
carries refs back to records.

**Derived** - anything the server computed and could compute again: indexes,
events, summaries. Your records are the opposite. They are the only data that
can't be regenerated, and backups exist to protect them. See
[Backup & restore](/docs/operate/backup-and-restore).
