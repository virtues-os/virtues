---
title: Reaching your server
description: How your phone and laptop connect to your Virtues server at home and away - pairing devices, what the relay can and cannot see, and what to expect when the network changes.
updated: 2026-10-20
---

Your server sits at home behind a router that, by default, blocks incoming
connections from the internet. You should be able to reach it the same way
from your kitchen and from another continent, without opening a port on your
home network. This page explains how that works.

## How connections work

Every device and every server has its own cryptographic key, and that key is
its identity. There is no password, account cookie or bearer token to steal.
Your phone dials your server by its key, and the server answers only devices
whose keys are on its allowlist.

The connection tries three paths in order, and you don't have to choose:

1. **Direct on your own network.** A phone and server on the same Wi-Fi talk
   straight to each other, and the traffic stays on your network.
2. **Direct across the internet.** The two ends punch through their routers
   and connect to each other without a middleman.
3. **Through a relay**, when the network won't allow a direct connection.
   Some corporate and mobile networks block it. The connection switches to a
   direct path if one becomes possible later.

**The server never opens an inbound port at home.** The server makes outgoing
connections only, so this works on a normal home router with no configuration
and no port forwarding.

## What you connect with

Use the iPhone app or the desktop app. Both use the key-based protocol. A
plain web browser can't, because it has no key to prove it's yours, so the
server refuses a browser on another machine, even one on your own network.

The one exception is a browser running on the server itself: the server
trusts requests from its own loopback address, so `http://localhost:8000` on
the server works without pairing. Otherwise, use the apps or a terminal on the
server.

## Pairing a device

Pairing puts a device's key on the server's allowlist. Which method you use
depends on where you are.

**Your first phone, during setup**, connects over Bluetooth. The server shows a
four-word phrase on its screen, and the phone has to send it back before the
server will do anything. This proves you can see the machine, so it works
without a shared secret.

**Any later device** pairs with a code from the server:

```bash
virtues pair
```

That prints a code to type into the app, then waits. On a server you've
already claimed, each code is new and works once. If your phone is on the same
network as the server, the app can scan a QR code instead.

**A phone joining from a different network** can get its identity from a
laptop that's already paired, on the Devices screen. The phone scans one QR
code and is paired, with no network path needed between the two devices.

> That handoff QR code **contains a private key**. Anyone who photographs it
> while it's on screen gets access to your server. Don't display it on a
> shared screen or a video call. If you think someone captured it, revoke the
> device right away. It appears in Devices as soon as it pairs.

To see and manage what's connected:

```bash
virtues device ls          # every device allowed to reach this server
virtues device rm <id>     # revoke one; its next connection is refused
```

Revocation takes effect immediately. The key stops working the next time the
device tries to connect.

## What the relay can and cannot see

When a direct path isn't possible, traffic passes through a relay we run.

- **It cannot read your traffic.** The connection is encrypted end-to-end
  between your device and your server, with keys that never reach the relay.
  The relay forwards each packet without decrypting it.
- **It can see** which two device keys are talking to each other, the IP
  addresses they connect from, and how much traffic passes and when. Any relay
  has to see volume and timing to forward packets.
- **It doesn't require an account.** The relay accepts connections from
  anyone: no account, subscription or sign-in. It protects itself with rate
  limits. You don't pay anything extra to reach your own server.

We don't describe the relay as "blind", because it sees the metadata above.
The claim that we can't read your data depends on the end-to-end encryption,
and doesn't depend on what the relay logs.

Your server ships with the relay's address built in, so you can reach it from
the first boot without an account. Most connected products ship their
rendezvous servers the same way. You can turn the relay off: Settings → System
→ Network shows the relay in use and has the switch. With the relay off, the
server still works on your own network.

## Checking the connection

```bash
virtues doctor
```

Among other things, this prints a reach summary: whether the server has an
identity, whether it knows a relay or is local-network-only, and how many
devices are paired.

## What to expect when the network changes

These behaviors are normal and don't indicate a fault:

- **After your phone sleeps, or moves between Wi-Fi and cellular**, the app
  rebuilds the connection automatically. This usually takes a second or two,
  and occasionally longer if it has to start from scratch.
- **A phone in the background closes its connection** so the radio can idle
  and save battery. It doesn't stay connected all the time.
- **A newly set up server** can take a few minutes to become reachable from
  outside your home while it connects to a relay.
- **On restrictive networks**, such as some workplaces and some mobile
  carriers, the direct path fails and all traffic goes through the relay. This
  is slower, and it is expected.
- **Guest and coworking Wi-Fi that isolates clients** stops devices on the
  same network from seeing each other. Use the handoff QR code above to pair
  in that case.

If the server itself seems unhealthy rather than unreachable, start with
[When something breaks](/docs/operate/recovery).
