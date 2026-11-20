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

The button in the bottom right corner of a new chat does the same things
from home: the arrows show the picture before or after for the rest of the
day, **Keep this picture** makes today's picture your setting, and the row
of collections switches between them. It also sets Light, Dark, Match
system or Follow the day, which is Light from sunrise to sunset.

## Next steps

Read [Reaching your server](../operate/reach.md) to see how your phone and
laptop load these pictures from your server.

---

## What your server downloads

The lake is part of Virtues. Every style is downloaded by your server the
first time you pick it, and the card shows its size before you do.

- **What is sent:** your server asks Virtues for the list of styles at most
  once a day, when an app opens the backgrounds, and asks for a style's
  archive when you pick it. Both requests carry your server's Virtues key,
  as every request to Virtues does, so Virtues can tell which account picked
  which style. They carry nothing from your records, and the service that
  answers them writes nothing about them to its logs.
- **What comes back:** one archive per style, about 100 MB, the same for
  everyone. Your server checks its size and checksum before it installs
  anything, and refuses an archive that does not match.
- **Where it goes:** `/var/lib/virtues/backgrounds/<style>` on your server.
  Your phone and laptop load the pictures from your server only, never from
  Virtues.

A downloaded style stays on your server until you delete its folder. A
server with no Virtues subscription offers Plain only, and home shows the
lake.
