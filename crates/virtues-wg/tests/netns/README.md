# netns auto-heal harness (Phase 6)

Validates the kernel-WireGuard transport the remote-access design depends on:
when the box's public endpoint moves, re-pointing a WireGuard peer endpoint is
enough to recover a live tunnel — no relay. (v1 recovers a moved box by
re-pairing rather than auto re-pointing; this harness still isolates the raw
transport behavior so we know kernel WG itself does the right thing.)

## What it does

`run.sh` builds two network namespaces (`box`, `client`) joined by a veth pair,
runs a real **kernel WireGuard** tunnel between them, and for each failure mode
asserts: **tunnel up → mutation breaks it → applying the new endpoint heals it.**

| Scenario | Models | Mutation |
|---|---|---|
| `prefix-rotation` | ISP hands out a new IPv6 prefix | box underlay v6 → new prefix |
| `nat-port-change` | reachable UDP port moves | box `listen-port` changes |
| `isp-swap` | box lands on a different network | new prefix **and** new port |

This harness isolates the kernel-WireGuard transport behavior the design depends
on (endpoint re-pointing recovers a live tunnel).

## Running

Linux, root, and the kernel `wireguard` module are required:

```sh
sudo crates/virtues-wg/tests/netns/run.sh
```

On non-Linux, non-root, or a kernel without WireGuard it prints `SKIP:` and
exits 0 — so cross-platform CI stays green while a dedicated Linux job runs it
for real. Exit code is non-zero only on an actual assertion failure.

## CI

Wire into a Linux runner with `NET_ADMIN` (the kernel module is present on
GitHub's `ubuntu-latest`):

```yaml
netns-autoheal:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - run: sudo modprobe wireguard && sudo crates/virtues-wg/tests/netns/run.sh
```
