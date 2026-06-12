# Bring Your Own Networking

Virtues' box is a real computer on the real internet. By default you reach it
**directly over IPv6 + the built-in WireGuard tunnel** (`virtues link` →
pairing). But the box doesn't care *how* a request arrives — **auth lives at the
app layer** ([`virtues-core/src/middleware/auth.rs`](../virtues-core/src/middleware/auth.rs)),
so the network is a dumb transport. That means you can put your box on *any*
networking you want, and it just works.

**Virtues never runs or requires an overlay.** The default is IPv6-direct.
Everything below is *your* infrastructure, *your* choice — we're never in the
path.

## When to reach for BYO

- Your ISP gives you no usable **global IPv6** (the built-in WG refuses to
  advertise a NAT/CGNAT/ULA address, so direct can't work — run
  `virtues doctor` to see your network class), **or**
- You already run an overlay (Tailscale, Headscale, a WireGuard VPS) and want
  the box on it.

## The three invariants

Any transport works if all three hold. They're already true in Virtues; a BYO
setup just has to satisfy the same three:

1. **The box answers on the interface.** The box binds `[::]:8000` (dual-stack)
   — it answers on *every* address it has: the WG tunnel, a Tailscale `100.x`,
   a global IPv6, a VPS-routed IP, whatever. No change needed.
2. **Your device targets the box's BYO address** (the `100.x`, the IPv6, the
   onion, the VPS IP) instead of the built-in WG address.
3. **Your device sends `Authorization: Bearer <device-token>`.** That token —
   minted when you pair a device — is the **only** thing that authenticates you.
   The pipe is never the trust boundary.

> **Privacy:** with WireGuard-based transports (Tailscale, Headscale, Netbird,
> Nebula, a plain-WG VPS) and direct IPv6, traffic is **end-to-end encrypted**
> and no third party ever sees plaintext. **Cloudflare Tunnel and Tor are
> different** — see their sections for the tradeoff.

## Virtues notices your overlay

`virtues doctor`, `virtues status --json`, and the dashboard's remote-access
item auto-detect a user-run overlay interface (`tailscale0`, a foreign `wg`,
netbird/nebula/zerotier) and report reachability **"via your own network"**.
Detection is report-only — Virtues never starts or configures the overlay.

## Composability at a glance

| Option | Works today? | Third party sees your traffic? |
|---|---|---|
| Direct IPv6 + Dynamic DNS | ✅ (the purest path) | No — nobody |
| Tailscale | ✅ as-is | No (E2E WireGuard; Tailscale relays only ciphertext) |
| Headscale (self-hosted) | ✅ as-is | No |
| Plain WireGuard + a $5 VPS | ✅ as-is | No (VPS forwards ciphertext) |
| Netbird / Nebula | ✅ as-is | No |
| Cloudflare Tunnel | ⚠️ works, but… | **Yes — Cloudflare terminates TLS** |
| Tor hidden service | ⚠️ works, but… | No, but high latency + needs a SOCKS client |

---

## Direct IPv6 + Dynamic DNS (recommended — no overlay at all)

The purest path: the box answers on its own global IPv6, and a DDNS client
keeps a name pointed at it as your ISP rotates the prefix.

```bash
# Keep an AAAA record pointed at the box (example: a provider with an update URL)
*/5 * * * * curl -fsS "https://dyndns.example/update?host=box.example.com&myip=$(curl -6 -fsS https://ifconfig.co)"
```

Open inbound **UDP/51820** (WireGuard) on your router/firewall (the box tries to
open it automatically; verify with `virtues doctor`). If your box knows its WAN
prefix out-of-band, set `VIRTUES_WG_PUBLIC_IP` so the WG daemon advertises it.
Then pair as normal — your devices reach the box at its global IPv6.

## Tailscale

```bash
# On the box AND on each device:
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up
```

Find the box's tailnet name (`tailscale status`), then point your device at
`http://<box-name>.<tailnet>.ts.net:8000` with your bearer token. The box
already answers there — no Virtues-side change. WireGuard-grade E2E encryption;
Tailscale's DERP relays (used only when a direct path can't form) forward
ciphertext only.

## Headscale (self-hosted Tailscale control plane)

Byte-identical to Tailscale from the box's view — only the control server moves
to a machine you run:

```bash
# On your server:
headscale serve                       # + a config.yaml and a created user
# On the box and devices:
sudo tailscale up --login-server=https://headscale.example.com
```

Target `http://<box>:8000` at its Headscale-assigned IP + bearer. Nothing in
Virtues knows or cares which control plane issued the tailnet.

## Plain WireGuard + a $5 VPS jump host

The best doctrine-honoring fallback when you have **no global IPv6**: a cheap VPS
that *does* have a public IP routes between your box and your devices. Traffic is
WireGuard-encrypted end-to-end; the VPS forwards ciphertext, never plaintext.

```ini
# VPS /etc/wireguard/wg0.conf  (the hub)
[Interface]
Address = 10.9.0.1/24
ListenPort = 51820
PrivateKey = <vps-priv>
PostUp = sysctl -w net.ipv4.ip_forward=1

[Peer]   # the box
PublicKey = <box-pub>
AllowedIPs = 10.9.0.2/32

[Peer]   # your laptop / phone
PublicKey = <laptop-pub>
AllowedIPs = 10.9.0.3/32
```

The box and each device peer with the VPS
(`Endpoint = <vps-ip>:51820`, `AllowedIPs = 10.9.0.0/24`,
`PersistentKeepalive = 25`). Then target `http://10.9.0.2:8000` + bearer.

## Netbird / Nebula

Both are self-hostable WireGuard/Noise meshes that hand the box an overlay IP on
a new interface — same story as Tailscale. Point your device at the overlay IP +
bearer. (Nebula needs you to run a lighthouse; Netbird needs its control
server.) No Virtues code touches it.

## Cloudflare Tunnel (last resort — read the tradeoff)

The box dials *out* to Cloudflare, which publishes a hostname. Useful when you
have **no inbound path at all**.

```bash
cloudflared tunnel login
cloudflared tunnel create virtues
cloudflared tunnel route dns virtues box.example.com
cloudflared tunnel run --url http://localhost:8000 virtues
```

Reach `https://box.example.com` + bearer.

> **Tradeoff:** Cloudflare **terminates TLS and can see your traffic in
> plaintext** — this breaks the privacy posture of every other option here. Your
> bearer still keeps strangers out, but Cloudflare is now in the middle. Latency
> is higher too (extra hop to CF's edge). Prefer a WireGuard-based option above.

## Tor hidden service (niche)

```
# On the box, in torrc:
HiddenServiceDir /var/lib/tor/virtues/
HiddenServicePort 8000 127.0.0.1:8000
```

A Tor-capable client reaches `http://<onion>.onion:8000` + bearer. Works because
auth is app-layer. Caveats: **high latency** (multi-hop), and you need a
SOCKS-capable client (CLI/curl/iOS-with-Orbot) — the desktop reverse proxy can't
dial `.onion` directly.

---

## Notes on the clients

- **iOS / Mac collector:** both take a **configurable box URL** and send a
  device bearer, so any BYO address works today — just point them at the box's
  BYO address.
- **Browsers:** a browser gets a clean Secure-Context origin at
  `http://localhost:8000` via the desktop client's local proxy, or hits the box
  over the built-in WG tunnel. Reaching the box's **raw** BYO IP from a browser
  (e.g. `http://100.x:8000`) is not a Secure Context, so cookie-based browser
  auth won't work there — use the desktop client (it proxies to a localhost
  Secure Context) or a bearer-based client. Daemon/app clients (which use
  bearers) are unaffected.
- **Desktop client over BYO:** the local proxy is independent of the built-in WG
  tunnel — it forwards `localhost:8000` to whatever upstream TCP address it's
  given. `virtues-client up --no-tunnel --upstream 100.x.y.z:8000` runs the
  localhost proxy over your own transport without bringing up Virtues'
  built-in WG.
