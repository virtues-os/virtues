---
title: Backgrounds
description: Choose the picture behind home in Virtues - a style that changes each day, one collection, one picture, or none - and what your server downloads when you pick a style.
updated: 2026-11-20
---

The picture behind home is a setting. It follows you to every device, and
each picture has a version for Light and one for Dark.

1. Open **Settings › Appearance › Background**.
2. Pick **Plain** for no picture, or a style. Until you pick one, home shows
   the lake that Virtues starts with.
3. Under the style, pick **Changes daily** or one collection. Under a
   collection, pick **Changes daily** or one picture.

A choice that stops above one picture shows a different picture each day.
It changes at midnight and does not repeat until every picture in it has
shown.

**Dots** is drawn by the app itself, so it needs no download. Its dots
drift, swell or fall slowly in the color of your text, and stay still when
Reduce motion is on.

The button in the bottom right corner of a new chat does the same things
from home. When your background changes daily, the arrows show the picture
before or after for the rest of the day, and **Keep this picture** makes
today's picture your setting. When you chose one picture, the arrows choose
its neighbor in the collection instead. The row of collections switches
between them. The button also sets Light, Dark, Match system or Follow the
day, which is Light from sunrise to sunset.

## Next steps

Read [Reaching your server](../operate/reach.md) to see how your phone and
laptop load these pictures from your server.

---

## What your server downloads

The lake and Dots are part of Virtues. Every other style is downloaded by
your server the first time you pick it, and the card shows its size before
you do.

- **What is sent:** when an app opens the backgrounds, your server asks
  Virtues for the list of styles if its copy is a day old, and at most once
  an hour while it cannot get one. It asks for a style's archive when you
  pick the style, and again when Virtues publishes a newer version of a
  style you have. These requests carry your server's Virtues key, as every
  request to Virtues does, so Virtues can tell which account downloaded
  which style. They carry nothing from your records, and the service that
  answers them writes nothing about them to its logs.
- **What comes back:** one archive per style, about 100 MB, the same for
  everyone. Your server checks its size and checksum before it installs
  anything, and refuses an archive that does not match.
- **Where it goes:** `/var/lib/virtues/backgrounds/<style>` on your server.
  Your phone and laptop load the pictures from your server only, never from
  Virtues.

A downloaded style stays on your server until you delete its folder, and
keeps showing while a newer version downloads. A server with no Virtues
subscription cannot download styles; it keeps the ones it has and offers
Plain.
