---
title: Mac
description: Pair a Mac with your Virtues server, as your first device or a later one.
updated: 2026-11-15
---

The Virtues desktop app needs macOS 26.2 or later.

Note: to pair, your Mac needs to be on the same network as your server, such
as your home Wi-Fi. After pairing, your Mac can reach your server from
anywhere.

## Pair your Mac as your first device

> Not yet tested on real hardware.

1. On your Mac, download the Virtues desktop app from
   [virtues.com/downloads](https://virtues.com/downloads), and open it.
   Setup opens. Choose **Begin**.
2. At **Find your server**, the app looks for your server on your network.
   If it finds one, it opens **Connect to your server** with the address
   already filled in. If it finds more than one, choose yours. If it finds
   none, choose **Enter address and code**.
3. Enter the pairing code the installer printed, then choose **Connect**.
   You can also paste a whole address line from the installer's output into
   **Address**, which fills in both fields. If the app says the code didn't
   work, it may have expired: run `virtues pair` on your server for a new
   one.
4. When it connects, your server's name appears for a moment. Then name
   your assistant, say what it should call you, and choose your home.
5. At **What this Mac can share**, choose **Collect from this Mac**, then
   **Open Full Disk Access**. System Settings opens with `virtues-collector`
   ready to drag into the list. macOS asks for Touch ID or your password to
   add it. Your Mac starts sending your messages and browsing to your server
   right away, before you choose a subscription.
6. At **Choose how … thinks**, keep **A Virtues subscription** chosen,
   choose **Continue to checkout**, and confirm in your browser. If you
   already have an account, choose **I already have an account**. To use
   your own AI model, choose **Your own AI** and enter its address and key.
7. Choose **Open Virtues**. **A note from us** opens the founder's letter,
   which you can also read later from Setup in the app.

If you installed over SSH and your network stops devices from seeing each
other, use the `ssh -L` command the installer printed.

---

## Pair your Mac as a later device

If your server already has a paired device, run `virtues pair` on your server
for a code, then pair the same way as above. After pairing, Setup opens
**Add your devices** for this Mac, and the app opens once you move on.
