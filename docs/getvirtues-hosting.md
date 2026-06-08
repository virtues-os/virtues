# `get.virtues.com` — what's needed to ship the curl-install path

The repo already contains:

- `scripts/install.sh` — the install script (Linux native, Avahi/mDNS, per-box TLS, setup-token)
- `.github/workflows/release-linux.yml` — GitHub Actions that cross-compiles
  `virtues` to `x86_64` + `aarch64` Linux on tag push, packages tarballs +
  SHA256 sidecars, and attaches them to a GitHub Release

What's NOT yet in place is the public hosting for the install script itself.
You're not blocked on it — anyone can install today with:

```sh
curl -sSL https://raw.githubusercontent.com/virtues-os/virtues/main/scripts/install.sh | sudo sh
```

…but for the marketed one-liner `curl get.virtues.com | sudo sh`, you need
to set up DNS + a small site that serves the script.

---

## Hosting options for `get.virtues.com`

Pick one. Cost / effort / ergonomics are all roughly the same; they differ in
who you trust.

### Option A — Cloudflare Pages  (recommended)

1. Sign in to Cloudflare, add `virtues.com` as a zone if it isn't already.
2. Create a new Pages project from this repo's `scripts/` directory:
   ```
   Build command:    (none — we serve install.sh as-is)
   Build output:     scripts
   ```
3. After deployment, add a custom domain: `get.virtues.com`.
4. (Optional) Add a `_redirects` file to `scripts/` so the index route
   serves the install script:
   ```
   /          /install.sh   200
   ```
5. Verify:
   ```sh
   curl -sSL https://get.virtues.com | head -3
   # should print the install.sh shebang + header
   ```

**Cost:** free for typical install-script traffic.
**Why this:** fastest CDN, custom domain trivial, no GitHub-rate-limit
exposure on the script itself (binaries still come from GitHub Releases —
but those have their own much higher rate limits).

### Option B — GitHub Pages

1. Enable Pages on `virtues-os/virtues` repo, source = `main` branch,
   directory = `/scripts`.
2. In the GitHub Pages settings, set a custom domain `get.virtues.com`.
3. Add a `CNAME` record at your DNS provider pointing `get.virtues.com`
   → `<owner>.github.io`.
4. Optional: add a tiny `index.html` redirect or just point the Pages
   project at `install.sh` directly.

**Cost:** free.
**Why this:** one less third party. Trade-off: GitHub Pages has lower
soft rate limits than Cloudflare.

### Option C — Nothing — use raw.githubusercontent.com

Skip the curl-friendly domain for v1, ship the README with the raw URL:

```sh
curl -sSL https://raw.githubusercontent.com/virtues-os/virtues/main/scripts/install.sh | sudo sh
```

**Cost:** $0.
**Why this:** zero infra, can ship today. UX cost: longer command, GitHub
brand visible.

---

## End-to-end flow once everything's wired

```
1. You push a tag:
   git tag v0.1.0 && git push origin v0.1.0

2. .github/workflows/release-linux.yml fires:
   - cross build for x86_64 + aarch64
   - tarballs + SHA256 sidecars
   - draft release with both arches uploaded
   - publish job flips it live

3. GitHub Releases now hosts:
   https://github.com/virtues-os/virtues/releases/download/v0.1.0/virtues-v0.1.0-x86_64-linux.tar.gz
   https://github.com/virtues-os/virtues/releases/download/v0.1.0/virtues-v0.1.0-aarch64-linux.tar.gz

4. User runs:
   curl -sSL https://get.virtues.com | sudo sh

5. install.sh:
   - hits https://api.github.com/repos/virtues-os/virtues/releases/latest → v0.1.0
   - downloads the matching arch tarball + sha256
   - verifies SHA256
   - installs Postgres / WireGuard / Avahi
   - sets hostname to `virtues`
   - drops mDNS service file
   - creates the system user + DB
   - installs the binary + systemd unit
   - prints next steps
```

---

## First-release checklist

When you're ready to ship the first `v0.1.0`:

- [ ] Pick Cloudflare Pages (A), GitHub Pages (B), or skip (C)
- [ ] Set up DNS for `get.virtues.com` if you went with A/B
- [ ] Verify `curl -sSL https://get.virtues.com | head -3` returns the install.sh
- [ ] Tag the repo: `git tag v0.1.0 && git push origin v0.1.0`
- [ ] Wait for GitHub Actions to finish (~10 min for both arches)
- [ ] Confirm the draft release shows both tarballs + SHA256 sidecars
- [ ] Confirm the publish job flipped it live
- [ ] Test on a real Linux box:
  ```sh
  curl -sSL https://get.virtues.com | sudo sh
  ```

## Gotchas worth knowing

1. **Distro Postgres versions.** Install script wants `postgresql-18 + postgresql-18-pgvector`.
   On Debian < 13 or Ubuntu < 26.04, Postgres 18 isn't in the default repos.
   Future enhancement to `install.sh`: detect this and add the PGDG repo.
2. **`cross` and OpenSSL.** If any crate links against system OpenSSL,
   `cross` needs the cross-arch lib mounted. We use rustls everywhere
   intentionally to avoid this. If a future dep pulls in `openssl-sys` we'd
   need to fix it before the release pipeline still works.
3. **Sqlx offline.** Workflow sets `SQLX_OFFLINE=true` and relies on the
   committed `.sqlx/` cache. Whenever queries change, run
   `cargo sqlx prepare --workspace` before the release tag.
4. **Binary size.** `virtues` is ~150 MB at debug, expect ~50 MB at release.
   GitHub Release tarballs are well under the 2 GB asset limit; no issue.
5. **First-time GitHub Actions runners on aarch64.** We're using
   `runs-on: ubuntu-latest` + cross-compile, NOT a native ARM runner.
   `cross` handles the linker. If you ever want native ARM (faster builds),
   GitHub now offers `ubuntu-24.04-arm` runners — drop `cross` and rebuild
   directly. Not worth the change at v1.
