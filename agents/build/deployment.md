# Deployment & Runtime Architecture

> How Virtues ships and runs. Companion to
> [`open-relay.md`](../record/open-relay.md) (how a paired device reaches the
> box) and [`entitlement.md`](entitlement.md) (the cloud wall).

---

## The model in one sentence

**Two shipping shapes — native Linux binary for the home box, Docker images on one server for the cloud — and nothing else.**

| Tier | What it is | How it ships | Privilege |
|---|---|---|---|
| **Home box** (DIY + appliance) | `virtues` binary (+ `virtues-qnnd` on NPU boards, or `virtues-embed` / `virtues-rerank` llama-server sidecars) | `curl -sSL https://virtues.com/sh \| sudo sh` → systemd units | Runs as the `virtues` user; see [Privilege](#privilege) |
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

`releases/latest` is whichever release GitHub flags Latest. `release-linux.yml` claims that flag explicitly for a stable box tag, and the Mac, Windows and Linux-desktop workflows publish with `make_latest: false`, so the one-liner serves the newest stable box release and never a prerelease. Testers opt into prereleases with `virtues upgrade --pre` (or `virtues.com/sh-pre`).

Bootstrap downloads the platform-specific `virtues-installer` binary from the latest GitHub Release, sha-verifies it, and execs it. The installer is idempotent; its order is `tools/virtues-installer/src/flow.rs`:

1. **System packages.** `apt` (Debian/Ubuntu, adding the PGDG repo) or `dnf` (Fedora): Postgres 18 + pgvector, `avahi-daemon`, `avahi-utils`, `libnss-mdns`.
2. **mDNS.** Set the hostname to `virtues` (skip with `VIRTUES_KEEP_HOSTNAME=1`) and drop `/etc/avahi/services/virtues.service`, advertising `_http._tcp` on port 8000.
3. **Appliance only:** move the fresh Postgres cluster onto the data disk before it holds anything.
4. **User and database.** Create the `virtues` system user and the data dir (default `/var/lib/virtues`), then the Postgres role, database, `vector` extension and the NOLOGIN separation roles the migrations expect.
5. **Binary.** Resolve the release, verify the tarball against its `.sha256`, stage it into a release slot under `/usr/local/share/virtues/releases/` and flip the `current` link (`download.rs`; `virtues upgrade` and `rollback` work within that layout).
6. **Local inference.** The QNN NPU daemon (`virtues-qnnd`) on a Dragon, the CPU llama-server sidecars (`virtues-embed`, `virtues-rerank`) in bundled mode, nothing for manual endpoints. Then libpdfium.
7. **Config.** Write the install manifest and the env file `<data dir>/virtues.env` (generating `VIRTUES_ENCRYPTION_KEY` only if it is absent), run migrations, and write `/etc/systemd/system/virtues.service` (`User=virtues`, `EnvironmentFile=-<data dir>/virtues.env`, `server --port 8000`).
8. **Appliance only:** the first-boot unit (installed with the service unit), then the appliance profile: kiosk display unit, bluetooth for BLE provisioning, unattended security upgrades.
9. **Start** with `systemctl enable` + `restart`, so a reinstall replaces a running old binary. Run the health check, then exec `virtues init` as the `virtues` user (skipped with `--no-init`).

No `make` commands and no `.env` editing. Subscription is opt-in and decoupled from install.

### Why native and not containers

The earlier plan was Podman + Quadlet on the appliance (rationale was "rootless + self-healing units"). That was abandoned because:

- **Quadlet adds a layer we don't need.** systemd already gives us restart-on-failure, dependency ordering, and per-unit security hardening (`ProtectSystem=`, `PrivateTmp=`, `CapabilityBoundingSet=`). Wrapping that in Podman wrappers in Quadlet wrappers in systemd was tower-of-leaky-abstractions.
- **Native Postgres is simpler.** Postgres-in-container needs volume mounts, init hooks, healthchecks; native Postgres is one apt package with a vendor-maintained systemd unit.
- **The box drives its own host.** Upgrades flip release slots, the appliance claims disks and runs a display, and the web terminal is a shell on the machine. A container would have to be granted all of that back.

The container trade-off makes sense in the cloud (multi-tenant, immutable infra, deploy via image push). It doesn't make sense on a single-tenant box you own.

---

## Privilege

`virtues.service` runs as `User=virtues` with no capabilities, and so do the
inference sidecars. Reach is iroh over a relay: the box dials outbound, so no
component needs `NET_ADMIN` or `/dev/net/tun`.

That is not the same as unprivileged. The installer grants the `virtues` user
passwordless sudo (`/etc/sudoers.d/virtues`), because the account has no
password and the owner's admin shell is the auth-gated web terminal running as
that user; the unit sets `NoNewPrivileges=false` so that works. The boundary is
therefore authentication to the box, not the uid. Code the box did not ship
(imported applets) runs under `systemd-run` with `NoNewPrivileges=yes` so it
cannot take that route to root; see [architecture.md](architecture.md#dispatch).

`cli/upgrade.rs` disables and removes a leftover `virtues-wireguard.service`
from boxes upgrading off a build that had one; keep it until no such box
remains.

---

## Networking: mDNS on the LAN

`avahi-daemon` is a stock distro package the installer enables, and the
installer drops `/etc/avahi/services/virtues.service` so the box advertises
itself as `_http._tcp` on port 8000 at `virtues.local`. Discovery is therefore
Avahi's job, not ours — there is no Virtues-owned multicast code. The app binds
a single explicit port (`:8000` HTTP, no TLS surface), unicast only.

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

## Installer env divergence

Dev and CI both provision Postgres more generously than a box does, so an
install-path bug can pass both and still stop a real box from booting:

| where | the `virtues` role | separation roles (`virtues_face_reader`, `virtues_applet_writer`) |
|---|---|---|
| dev (`Makefile`) | `LOGIN CREATEDB CREATEROLE` | pre-created |
| CI (`ci.yml`) | the service's superuser | pre-created by a setup step |
| a box (`install.rs`) | `createuser --no-createrole` | pre-created as `postgres` by `provision_separation_roles()`, `WITH ADMIN OPTION` |

A migration that needs a cluster privilege the box's role lacks passes locally
and in CI and then aborts at pool connect on a box, which exits the server. So
green CI is not evidence the installer works, and when an install-path failure
shows up in CI, fix the installer, not the workflow. A new cluster role goes
into `provision_separation_roles()` in the same change as its migration.

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
- `deploy/wireguard.Dockerfile`, `crates/virtues-wg`, and any privileged
  networking daemon — reach is iroh over the relay
- Nomad job files (gone — replaced by `docker run` on the cloud server)

The cloud `services/{atlas,virtues-api}/Dockerfile` are the only Dockerfiles that still matter.
