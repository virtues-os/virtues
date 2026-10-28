---
title: Windows
description: Pair a Windows PC with your Virtues server, as your first device or a later one.
updated: 2026-10-26
---

## Pair your PC as your first device

> Not yet tested on real hardware.

1. On your server, run `virtues pair`. It prints a six-digit pairing code.
   The code works until the command prompt (such as `user@server:~$`) comes
   back below it. Once it does, the code has expired: run `virtues pair`
   again.
2. Download the Virtues desktop app from
   [virtues.com/downloads](https://virtues.com/downloads), and open it.
3. Enter the six-digit code.

If the app doesn't find your server, type one of the addresses the installer
printed, such as `virtues.local` or your server's IP address. If you installed
over SSH and your network stops devices from seeing each other, use the
`ssh -L` command the installer printed.

---

## Pair your PC as a later device

If your PC isn't the first device you're pairing with your server, run
`virtues pair` on your server to show a new code, then enter it in the desktop
app, the same way as above.
