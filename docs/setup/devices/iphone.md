---
title: iPhone
description: Pair an iPhone with your Virtues server, as your first device or from a computer you've already paired.
updated: 2026-10-26
---

The Virtues iPhone app needs iOS 26.2 or later.

## Pair your iPhone as your first device

> These steps haven't been tested yet.

1. On your server, run `virtues pair`. It prints a six-digit pairing code.
   The code works until the command prompt (such as `user@server:~$`) comes
   back below it. Once it does, the code has expired: run `virtues pair`
   again.
2. Install the Virtues iPhone app from the App Store, and open it.
3. The app opens a QR code scanner. Tap **Find your server on this network**.
4. Enter the six-digit code.

If the app doesn't find your server, type one of the addresses the installer
printed, such as `virtues.local` or your server's IP address.

---

## Pair your iPhone from a paired computer

Open the Devices screen in the desktop app on your Mac or PC, and scan its QR
code with the iPhone app. This works even when your iPhone and server are on
different networks. Anyone who photographs that QR code gets access to your
server, so don't show it on a shared screen.
[Reaching your server](/docs/operate/reach#pairing-a-device) explains more.
