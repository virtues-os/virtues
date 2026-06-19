# Deployment & Runtime Architecture

> How Virtues ships and runs. Companion to
> [`networking.md`](networking.md) (the WG transport + IPv6-direct reachability)
> and [`entitlement.md`](entitlement.md) (the cloud wall).

---

## The model in one sentence

**Two shipping shapes — native Linux binary for the home box, Docker images on EC2 for the cloud — and nothing else.**

| Tier | What it is | How it ships | Privilege |
|---|---|---|---|
| **Home box** (DIY + appliance) | `virtues` binary + `virtues-wireguard` daemon | `curl -sSL https://virtues.com/sh \| sudo sh` → systemd units | App rootless; WG daemon `NET_ADMIN` only |
| **Cloud sidecar** (Virtues-operated) | `atlas` + `virtues-api` services | Docker images on a single EC2 + Caddy | `docker run`, no orchestrator |
| **Clients** | Web UI (SvelteKit), iOS app, Mac collector | Static site / App Store / signed pkg | None |

That's it. No Compose, no Quadlet, no Kubernetes, no Nomad anywhere in the product. The cloud sidecar is the only thing that runs in containers, and it's there because cloud → containers is the right call for stateless services behind a TLS terminator.

---

## Home box: native install via `tools/bootstrap.sh` + `virtues-installer`

`curl -sSL https://virtues.com/sh | sudo sh` runs the bootstrap, which is `tools/bootstrap.sh`. The website (`virtues.com`, SvelteKit on Vercel) serves `/sh` as a **302 redirect to the latest stable GitHub Release asset** (`bootstrap.sh`, uploaded by release-linux.yml), so the script and the binaries it fetches version together — gated on the same tag — and no server-side copy exists to drift. The canonical command uses `-sSL` precisely because the endpoint is a redirect (`-L` follows it). The website route (`src/routes/sh/+server.ts`) is just:

```ts
// virtues.com/sh
redirect(302, 'https://github.com/virtues-os/virtues/releases/latest/download/bootstrap.sh');
```

`releases/latest` resolves to the newest **stable** (non-prerelease) release, so the one-liner never serves an `edge` build. Testers opt into edge with `virtues upgrade --pre` (or `virtues.com/sh-pre`).

Bootstrap downloads the platform-specific `virtues-installer` binary from the latest GitHub Release, sha-verifies it, and execs it. The installer is idempotent and does, in order:

1. **Pick the package manager** (`apt` for Debian/Ubuntu, `dnf` for Fedora) and install: `postgresql-18` + `postgresql-18-pgvector`, `avahi-daemon`, `libnss-mdns`, `wireguard`, `openssl`, `ca-certificates`.
2. **Resolve the latest release tag** from `https://api.github.com/repos/virtues-os/virtues/releases/latest`, download the `virtues-<ver>-<arch>-linux.tar.gz` tarball + its `.sha256` from the matching GitHub Release, verify the checksum, extract the `virtues` binary to `/usr/local/bin/`.
3. **Set hostname** to `virtues` (skip with `VIRTUES_KEEP_HOSTNAME=1`) and drop `/etc/avahi/services/virtues.service` so the box advertises itself on the LAN as `_https._tcp` on `virtues.local`.
4. **Create the `virtues` system user** that owns the daemon, with a home at `/var/lib/virtues/`.
5. **Provision the database**: `createdb virtues` and `CREATE EXTENSION vector`. Generate a `VIRTUES_ENCRYPTION_KEY` if `/etc/virtues/env` doesn't already have one.
6. **Install + enable the systemd unit** `/etc/systemd/system/virtues.service` (runs `/usr/local/bin/virtues` as user `virtues`, with `EnvironmentFile=/etc/virtues/env`).
7. **Print next steps** — start the service, run `virtues link` for the login URL, optional `virtues subscribe`.

The user's only interactive step is opening the printed URL in a browser. No `make` commands. No `.env` editing. Subscription is opt-in and decoupled from install.

### Why native and not containers

The earlier plan was Podman + Quadlet on the appliance (rationale was "rootless + self-healing units"). That was abandoned because:

- **Quadlet adds a layer we don't need.** systemd already gives us restart-on-failure, dependency ordering, and per-unit security hardening (`ProtectSystem=`, `PrivateTmp=`, `CapabilityBoundingSet=`). Wrapping that in Podman wrappers in Quadlet wrappers in systemd was tower-of-leaky-abstractions.
- **Native Postgres is simpler.** Postgres-in-container needs volume mounts, init hooks, healthchecks; native Postgres is one apt package with a vendor-maintained systemd unit.
- **The privileged WG daemon doesn't benefit from containerization.** It needs `NET_ADMIN` against the host kernel either way; the bridge-vs-host networking dance Quadlet exists to manage is the *exact* problem we're avoiding by being on the host.
- **Distros we target ship modern enough kernels** (Debian 13 trixie ships kernel 6.10+) for kernel WireGuard, so we don't need userspace `wireguard-go` in a container.

The container trade-off makes sense in the cloud (multi-tenant, immutable infra, deploy via image push). It doesn't make sense on a single-tenant box you own.

---

## The privilege split: app rootless, WG daemon rootful

WireGuard needs `CAP_NET_ADMIN` against the kernel. Rather than make the *whole app* privileged, we isolate WG into a **minimal standalone daemon** so the privileged surface is tiny.

- **`crates/virtues-wg`** — depends only on `defguard_wireguard_rs` + `sqlx` + crypto. No web, no ML, no interpreters.
- **`virtues-wireguard`** binary (`virtues-wireguard.service`) — runs rootful with `NET_ADMIN` + `/dev/net/tun`. Owns: `wg0` lifecycle, **reconcile loop**, the netlink IPv6-change watcher (records the box's current endpoint for the pairing bundle), and the mDNS/SSDP multicast functions. Kernel WireGuard only — the host must have the `wireguard` module (shipped in the Jetson appliance image; stock on DIY mini-PCs).
- **`virtues`** binary (`virtues.service`) — runs as `User=virtues` (no capabilities). Web UI, API, ingestion, ML. Its only WG involvement is writing peer config rows to the DB.

**The DB is the interface — no IPC.** Pairing writes peers via `pairing::store_peer` (→ `credentials.metadata.wg`); the WG daemon reconciles `wg0` from `pairing::load_all_peers`. Prompt application of new pairings comes from Postgres `LISTEN/NOTIFY`.

*risk ≈ privilege × attack surface.* The privileged component is a few hundred KB of WG/netlink/DB code with no untrusted input beyond peer config from our own DB — vs. making the fat app (web + ML + Python interpreter) the privileged one.

---

## Networking: mDNS and SSDP live in the WG daemon

Multicast/broadcast protocols (mDNS for `virtues.local` discovery, SSDP for the pinhole wizard's router detection) require host network access. Both ride along with the WG daemon (which is already host-networked for `wg0`). The app process binds a single explicit port (`:8000` HTTP, no TLS surface — see [[localhost-daemon-trust]] in MEMORY.md), unicast only.

---

## Cloud sidecar: `atlas` + `virtues-api` on EC2

The cloud half is the *metered* edge — Stripe billing (atlas) and the AI/web/bank passthrough (virtues-api). Single tenant from a box's perspective; multi-tenant from the cloud's perspective. Shipped as:

- **Two Docker images** (`services/virtues-atlas/Dockerfile`, `services/virtues-api/Dockerfile`), built `--platform linux/amd64`, pushed to ECR via `make deploy-atlas` / `make deploy-virtues-api`.
- **One EC2 instance** runs both as `docker run` units behind **Caddy** (which terminates TLS for `atlas.virtues.com` + `api.virtues.com` via Let's Encrypt and reverse-proxies to the containers).
- **RDS Postgres** with TLS-to-RDS, in the same VPC. No NAT, no load balancer, no App Runner — flat and cheap.
- **Access**: SSM Session Manager only, no public SSH.

Why one EC2 + Caddy and not ECS/App Runner/Fargate: latency, cost, and avoiding NAT gateway charges for a small workload. The whole cloud half fits on one `t4g.medium` and reboots in <30s. If load demands it, we'll split.

---

## Release pipeline

**Home-box releases:**
- GitHub Actions workflow `release-linux.yml` builds the `virtues` binary on tag push (`v*`).
- Matrix: `x86_64-unknown-linux-gnu` + `aarch64-unknown-linux-gnu` via `cross`.
- Each tarball + sha256 uploads to the GitHub Release as a draft. A publish job flips draft → live after both arches are built.
- `tools/bootstrap.sh` discovers the latest release via the GitHub API at install time — no separate "manifest" or update server.

**Cloud releases:**
- `make deploy-atlas` / `make deploy-virtues-api` build + push `:latest` to ECR.
- The EC2 instance pulls the new `:latest` and restarts the container (manual today, candidate for a GitHub Action that SSH-runs `docker pull && docker restart` later).

---

## What you won't find in this repo (any more)

- `docker-compose.yml` (deleted — orphaned by native install)
- `deploy/quadlet/` (deleted — orphaned by native install)
- `deploy/wireguard.Dockerfile` (deleted — WG daemon ships as native binary alongside `virtues`)
- Nomad job files (gone — replaced by `docker run` on EC2)

The cloud `services/{atlas,virtues-api}/Dockerfile` are the only Dockerfiles that still matter.
