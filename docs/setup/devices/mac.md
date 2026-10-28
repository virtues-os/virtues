---
title: Mac
description: Pair a Mac with your Virtues server, as your first device or a later one.
updated: 2026-10-26
---

The Virtues desktop app needs macOS 26.2 or later.

Note: to pair, your Mac needs to be on the same network as your server, such
as your home Wi-Fi. After pairing, your Mac can reach your server from
anywhere.

## Pair your Mac as your first device

> Not yet tested on real hardware.

1. On your Mac, download the Virtues desktop app from
   [virtues.com/downloads](https://virtues.com/downloads), and open it.
2. At **Sign in to Virtues**, enter your email, then the six-digit code
   Virtues emails you. This is a different code from your server's pairing
   code. A new email creates your account, and you don't pay anything here.
   To skip the account and use your own AI model, choose
   **Use my own AI instead**.
3. When the search for Virtues hardware returns no results, choose
   **Join it instead**. (The search only finds hardware we build, not a server
   you installed yourself.)
4. On your server, run `virtues pair`. It prints a six-digit pairing code.
   The code expires if the command prompt (such as `user@server:~$`) comes
   back below it. If so, run `virtues pair` again.
5. On your Mac, enter your server's address and the pairing code in the app.
   Your Mac needs to be on the same network as your server for this step. For the address, use your server's IP address followed by `:8000`,
   such as `192.168.1.20:8000`. To find it, run `hostname -I` on your server.

If you installed over SSH and your network stops devices from seeing each
other, use the `ssh -L` command the installer printed.

---

## Pair your Mac as a later device

If your Mac isn't the first device you're pairing with your server, run
`virtues pair` on your server to show a new code, then enter it in the desktop
app, the same way as above.
