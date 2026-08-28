---
title: Reaching your server
description: How your phone and laptop connect to your Virtues box at home and away — pairing devices, what the relay can and cannot see, and what to expect when the network changes.
updated: 2026-08-28
---

Your box sits at home behind a router that, by default, nothing on the
internet can reach. Getting to it from your kitchen and from another continent
should feel the same, and it should not require opening a hole in your home
network. This is how that works.

## The shape of it

Every device and every box has its own cryptographic key. That key *is* its
identity — there's no password, no account cookie, no bearer token to steal.
Your phone dials your box by its key, and the box answers only devices whose
keys are on its allowlist.

Three paths, tried in order, all invisible to you:

1. **Direct on your own network.** Phone and box on the same Wi-Fi talk
   straight to each other. Nothing leaves the building.
2. **Direct across the internet.** The two ends punch through their routers
   and connect to each other without a middleman.
3. **Through a relay**, when the network won't allow a direct connection —
   some corporate and mobile networks won't. The connection upgrades itself to
   a direct path if one becomes possible.

**No inbound port is ever opened at home.** The box dials out; nothing dials
in. That's why this works on a normal home router with no configuration and no
port forwarding.

## What you connect with

The iPhone app and the desktop app. Both speak the key-based protocol; a
plain web browser can't, because it has no key to prove it's yours — a browser
pointed at the box on your own network is refused like any other stranger.

So: the apps, or a terminal on the box itself.

## Pairing a device

Pairing is putting a device's key on the box's allowlist. There are a few
routes, and which one you take depends on where you are.

**Your first phone, during setup**, connects over Bluetooth. The box shows a
four-word phrase on its screen and the phone has to send it back before the
box will do anything — proof you can see the machine, rather than proof you
know a secret.

**Any later device** pairs with a code from the box:

```bash
virtues pair
```

That prints a code to type into the app, then waits. On a box that's already
yours, each code is fresh and single-use. If your phone is on the same network
as the box, the app can scan a QR instead.

**A phone joining from a different network** can be handed its identity by a
laptop that's already paired, from the Devices screen — one scan and it's in,
with no network path needed between the two devices.

> That handoff QR **contains a private key**. Anyone who photographs it while
> it's on screen gets access to your box. Don't display it on a shared screen
> or a video call, and if you suspect someone caught it, revoke the device
> immediately — it appears in Devices the moment it pairs.

To see and manage what's connected:

```bash
virtues device ls          # every device allowed to reach this box
virtues device rm <id>     # revoke one; its next connection is refused
```

Revocation is immediate and total. The key stops working the next time it
tries.

## What the relay can and cannot see

When a direct path isn't possible, traffic passes through a relay we run. The
honest description:

- **It cannot read anything.** The connection is encrypted end-to-end between
  your device and your box, with keys the relay never holds. It forwards
  packets it has no ability to open.
- **It does see** which two device keys are talking to each other, the IP
  addresses they connect from, and how much traffic passes and when. That's
  unavoidable for anything that forwards packets — a relay that couldn't see
  volume and timing couldn't move the bytes.
- **It checks your subscription by key** before carrying anything for you.

We'd rather state that plainly than round it up to "blind." The strong claim —
that we can't read your life — is true and rests on the encryption, not on
promises about the relay's memory.

Note the consequence: **remote reach requires an active subscription.** If it
lapses, the relay stops carrying you, and your box keeps working normally on
your own network and keeps collecting your data. Local access doesn't depend
on us.

## Checking the connection

```bash
virtues doctor
```

Among other things this prints a reach summary: whether the box has an
identity, whether it knows a relay or is local-network-only, and how many
devices are paired.

## What to expect when the network changes

Some behavior is worth recognizing so it doesn't read as a fault:

- **After your phone sleeps, or moves between Wi-Fi and cellular**, the
  connection is rebuilt automatically. Usually a second or two; occasionally
  longer if it has to rebuild from scratch.
- **A backgrounded phone deliberately parks its connection** so the radio can
  idle and the battery lasts. It isn't continuously connected, by design.
- **A box that was just set up** can take a few minutes to become reachable
  from outside your home, as it settles into a relay.
- **On restrictive networks** — some workplaces, some mobile carriers — the
  direct path fails and everything rides the relay. That's slower, and
  expected rather than broken.
- **Guest and coworking Wi-Fi that isolates clients** blocks devices on the
  same network from seeing each other. The handoff QR above is the way through
  it.

If the box itself seems unhealthy rather than unreachable,
[When something breaks](/docs/operate/recovery) is the place to start.
