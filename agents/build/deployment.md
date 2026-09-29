# Deployment & Runtime Architecture

> How Virtues ships and runs. Companion to
> [`relay-control-plane.md`](../archive/relay-control-plane.md) (how a paired device reaches
> the box; `networking-relay-tee.md` is the superseded pre-iroh ADR)
> and [`entitlement.md`](entitlement.md) (the cloud wall).

---

## The model in one sentence

**Two shipping shapes — native Linux binary for the home box, Docker images on one server for the cloud — and nothing else.**

| Tier | What it is | How it ships | Privilege |
|---|---|---|---|
| **Home box** (DIY + appliance) | `virtues` binary (+ `virtues-qnnd` on NPU boards) | `curl -sSL https://virtues.com/sh \| sudo sh` → systemd units | Rootless throughout — no privileged component |
| **Cloud sidecar** (Virtues-operated) | `atlas` + `virtues-api` services | Docker images on one dedicated server + Caddy | `docker run`, no orchestrator |
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

1. **Pick the package manager** (`apt` for Debian/Ubuntu, `dnf` for Fedora) and install: `postgresql-18` + `postgresql-18-pgvector`, `avahi-daemon`, `avahi-utils`, `libnss-mdns`, `ca-certificates`, `curl`.
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

## Privilege: nothing is rootful

The box runs **one unprivileged service**. `virtues.service` runs as
`User=virtues` with no capabilities; on NPU boards `virtues-qnnd.service` joins
it, also unprivileged. There is no `NET_ADMIN` component and nothing that needs
`/dev/net/tun`.

> **This section used to describe a privilege split**, because reach was
> WireGuard: a minimal rootful `virtues-wireguard` daemon (`crates/virtues-wg`)
> owned `wg0`, the reconcile loop, the netlink IPv6 watcher, and the
> mDNS/SSDP multicast functions, coordinating with the rootless app through the
> DB rather than IPC. **All of it is gone.** Reach is iroh over a relay — the box
> dials *outbound* and needs no kernel networking privileges at all, which is
> what let the privileged component be deleted rather than merely shrunk. See
> [`relay-control-plane.md`](../archive/relay-control-plane.md).
>
> The only trace left in the codebase is retirement code: `cli/upgrade.rs`
> disables and removes a leftover `virtues-wireguard.service` on boxes upgrading
> from a build that had one. That is deliberate and should stay until no such
> box remains.

The security argument that motivated the split still holds in its stronger
form — *risk ≈ privilege × attack surface*, and the privilege term is now zero.

---

## Networking: mDNS on the LAN

`avahi-daemon` is a stock distro package the installer enables, and the
installer drops `/etc/avahi/services/virtues.service` so the box advertises
itself as `_https._tcp` on `virtues.local`. Discovery is therefore Avahi's job,
not ours — there is no Virtues-owned multicast code and no host-networked
process. The app binds a single explicit port (`:8000` HTTP, no TLS surface),
unicast only.

SSDP is gone with the pinhole wizard: there is no port to forward, so there is
no router to detect.

---

## Cloud sidecar: `atlas` + `virtues-api` on one server

The cloud half is the *metered* edge — Stripe billing (atlas) and the AI/web/bank passthrough (virtues-api), plus the map files boxes download. Single tenant from a box's perspective; multi-tenant from the cloud's perspective. Shipped as:

- **Two Docker images** (`services/virtues-atlas/Dockerfile`, `services/virtues-api/Dockerfile`), built **on the server** from a pushed commit — no registry sits in the path.
- **One dedicated server** runs both as `docker run --network host` units behind **Caddy**, which terminates TLS for `atlas.virtues.com` + `api.virtues.com` and reverse-proxies to the containers. Env lives in root-only files, `/etc/virtues/{atlas,api}.env`.
- **Postgres on the same server**, over the local socket. Continuous WAL archiving plus scheduled full and differential backups go to object storage off the server, encrypted; a restore was tested before any production data landed.
- **Maps**: a monthly systemd timer cuts the Protomaps build into `/srv/maps/<build>/` (`deploy/maps/`), mounted read-only into virtues-api, which serves it outside request tracing.
- **Access**: SSH by key only; the firewall opens 22, 80 and 443.

Why one server and not a managed platform: cost, unmetered bandwidth for the map files, and a workload small enough that one machine with backups off it is the honest shape. The relay runs on its own server so a deploy or a map download never touches remote access.

### Monitoring

Both servers run `virtues-health` every ten minutes and email the operator (through Resend) when something changes and once a day while it stays wrong. It checks RAID members, SMART health and wear, disk space, failed systemd units, both containers, the public `/health` endpoints, backup age and WAL archiving, and certificate expiry. Each server also checks the other's public endpoints, because a dead server cannot report itself. A failed backup or map cut emails immediately (`OnFailure=`), as does an `mdadm` RAID event. Docker logs are capped (`local` driver, 5 × 20 MB).

---

## Release pipeline

**Home-box releases:**
- GitHub Actions workflow `release-linux.yml` builds the `virtues` binary on tag push (`v*`).
- Matrix: `x86_64-unknown-linux-gnu` + `aarch64-unknown-linux-gnu` via `cross`.
- Each tarball + sha256 uploads to the GitHub Release as a draft. A publish job flips draft → live after both arches are built.
- `tools/bootstrap.sh` discovers the latest release via the GitHub API at install time — no separate "manifest" or update server.

**Cloud releases:**

```sh
make deploy-virtues-api REF=<sha|tag|branch>
make deploy-atlas REF=<sha|tag|branch>
make deploy-rollback SVC=virtues-api        # back to the image before the last deploy
```

`tools/deploy-service.sh` builds the image on the server from exactly that
commit (it must be pushed; a branch name means `origin/<branch>`), starts it on
a spare port against the live config, and swaps it in only if it answers
(`/ready` for virtues-api, `/health` for atlas). The replaced image is kept as
`<service>:previous`, and a container that does not come up is rolled back
automatically. One deploy per service runs at a time.

**The smoke test runs against the live database, and both services migrate on
startup.** A failed smoke test leaves the running service untouched, but a
migration the new image applied stays applied. Migrations are append-only, so
the old image keeps working on the newer schema — keep it that way.

**Env changes need a recreate.** `docker restart` re-runs the existing
container, so it neither picks up a new image nor re-reads the env file. After
editing `/etc/virtues/*.env`, redeploy the running ref (or `rm -f` + `run` with
the same flags the script uses).

**Check what a deploy changed.** After a model change in particular:

```sh
docker logs virtues-api 2>&1 | grep -i "model catalog"    # want: catalog loaded count=NNN
docker logs virtues-api 2>&1 | grep -ciE "ERROR|panic"    # want: 0
curl -s https://api.virtues.com/health                    # want: 200
```

A zero error count is the check that matters most on a model change: virtues-api
logs `SLOT DEFAULTS are NOT in the gateway catalog` at error level when a slot id
no longer exists upstream, which is the one failure that 404s every user we route
to it.

---

## What you won't find in this repo (any more)

- `docker-compose.yml` (deleted — orphaned by native install)
- `deploy/quadlet/` (deleted — orphaned by native install)
- `deploy/wireguard.Dockerfile` **and the WG daemon itself** (`crates/virtues-wg`,
  `virtues-wireguard.service`) — deleted with the move to the relay
- Nomad job files (gone — replaced by `docker run` on the cloud server)

The cloud `services/{atlas,virtues-api}/Dockerfile` are the only Dockerfiles that still matter.
