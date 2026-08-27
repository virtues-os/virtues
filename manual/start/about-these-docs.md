---
title: About these docs
description: How the Virtues documentation is versioned, where it lives, and how to read it as plain markdown — for people and for their agents.
updated: 2026-08-27
---

Three things about these docs are unusual enough to be worth a page.

## They version with the software

The manual's source lives in the
[main Virtues repository](https://github.com/virtues-os/virtues), in
[`manual/`](https://github.com/virtues-os/virtues/tree/main/manual) — the same
repository as the code it describes. A page about upgrading merges in the same
change as the upgrade behavior it documents, and this site publishes from a
pinned ref, so what you read here describes what a server actually runs — not
whatever was on a development branch this morning.

The server will eventually serve its own copy of the manual, matched exactly
to its installed version and readable offline. This site is the public
mirror — and the place you come when the server itself is what's broken.

## Every page is also plain markdown

Append `.md` to any docs URL to get the page as raw markdown —
[`/docs/start/about-these-docs.md`](/docs/start/about-these-docs.md) is this
page. Paste it into a model, pipe it through `curl`, or point an agent at it;
nothing is lost in rendering because the source *is* markdown.

For agents there's also [`/llms.txt`](/llms.txt) — an index of every published
page with descriptions — and [`/llms-full.txt`](/llms-full.txt), the entire
manual concatenated into one document.

## They claim only what ships

A page exists here when the thing it describes is real. Pages marked *soon* in
the sidebar are the honest table of contents — designed, sequenced, not yet
written, because the features they cover are still stabilizing. When you read
a published page, you can act on it.

The engineering notes behind these docs — design records, audits, the
reasoning that shaped decisions — are public too, in the repository's
[`docs/`](https://github.com/virtues-os/virtues/tree/main/docs) directory.
They're written for the people building Virtues rather than running it, and
they say so; read them for the *why* behind what these pages describe.
