# Deployment & Runtime Architecture

> How Virtues ships and runs. Companion to
> [`networking-relay-tee.md`](networking-relay-tee.md) (how a browser reaches the box,
> via the blind relay)
> and [`entitlement.md`](entitlement.md) (the cloud wall).

---

## The model in one sentence

**Two shipping shapes — native Linux binary for the home box, Docker images on EC2 for the cloud — and nothing else.**

| Tier | What it is | How it ships | Privilege |
|---|---|---|---|
| **Home box** (DIY + appliance) | `virtues` binary (+ `virtues-qnnd` on NPU boards) | `curl -sSL https://virtues.com/sh \| sudo sh` → systemd units | Rootless throughout — no privileged component |
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
> DB rather than IPC. **All of it is gone.** Reach is the blind relay — the box
> dials *outbound* and needs no kernel networking privileges at all, which is
> what let the privileged component be deleted rather than merely shrunk. See
> [`networking-relay-tee.md`](networking-relay-tee.md).
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
- The EC2 instance then pulls and **recreates** the container. Manual today;
  candidate for a GitHub Action later.

One EC2 instance runs both containers. Access is SSM only — no public SSH — so
step two is a `send-command`. The account ID and instance ID live in the private
ops note, not here; export them first:

```sh
ACCOUNT=<aws-account-id>            # private ops note
INSTANCE=<ec2-instance-id>          # private ops note
REGISTRY=$ACCOUNT.dkr.ecr.us-east-1.amazonaws.com
ECR=$REGISTRY/virtues-api:latest    # or virtues-atlas
aws ssm send-command --instance-ids $INSTANCE \
  --document-name AWS-RunShellScript --parameters "commands=[
    \"aws ecr get-login-password --region us-east-1 | docker login --username AWS --password-stdin $REGISTRY\",
    \"docker pull $ECR\",
    \"docker rm -f virtues-api\",
    \"docker run -d --name virtues-api --restart unless-stopped --network host --env-file /etc/virtues/api.env $ECR\",
    \"sleep 20\",
    \"docker inspect virtues-api --format 'image={{.Image}} health={{.State.Health.Status}}'\"]"
```

**`docker restart` is not enough, twice over:** it re-runs the *existing*
container, so it neither picks up the newly pulled image nor re-reads
`/etc/virtues/api.env`. It must be `rm -f` + `run`. The run flags above are not
optional decoration — they reconstruct the live container exactly (host network,
no port bindings, no binds, `--env-file` only, no stray `-e`).

**Verify the deploy actually changed something.** `:latest` deploys fail
silently by design — the image ID before and after is the only proof:

```sh
docker inspect virtues-api --format '{{.Image}}'          # before and after
docker logs virtues-api 2>&1 | grep -i "model catalog"    # want: catalog loaded count=NNN
docker logs virtues-api 2>&1 | grep -ciE "ERROR|panic"    # want: 0
curl -s https://api.virtues.com/health                    # want: 200
```

A zero error count is the check that matters most on a model change: virtues-api
logs `SLOT DEFAULTS are NOT in the gateway catalog` at error level when a slot id
no longer exists upstream, which is the one failure that 404s every user we route
to it. Keep the previous image ID to hand — `docker run` against it is the
rollback.

---

## What you won't find in this repo (any more)

- `docker-compose.yml` (deleted — orphaned by native install)
- `deploy/quadlet/` (deleted — orphaned by native install)
- `deploy/wireguard.Dockerfile` **and the WG daemon itself** (`crates/virtues-wg`,
  `virtues-wireguard.service`) — deleted with the move to the blind relay
- Nomad job files (gone — replaced by `docker run` on EC2)

The cloud `services/{atlas,virtues-api}/Dockerfile` are the only Dockerfiles that still matter.
