#!/bin/sh
# Virtues bootstrap — the tiny POSIX shell script that `virtues.com/sh`
# serves.
#
# What this script does:
#   1. Verifies Linux + amd64/arm64 + we're root
#   2. Resolves the latest virtues-installer release tag from GitHub
#   3. Downloads `virtues-installer-<tag>-<arch>-linux` + its `.sha256`
#   4. Verifies the SHA256 (defense-in-depth alongside HTTPS)
#   5. chmods + execs the installer with the user's original flags
#
# Everything past this script — TUI, progress, install logic — lives in
# the installer binary. Keeping bootstrap minimal + POSIX means: works
# under dash (Debian/Ubuntu's default /bin/sh when invoked via `| sh`),
# easy to audit via `curl virtues.com/sh | less`, fast to download,
# almost nothing to go wrong before we have a real UI to show.

# `pipefail` is bash-only; dash doesn't have it. Stick with -eu and
# guard pipelines explicitly (the only one is the latest-tag resolve
# which we re-check for empty output afterward).
set -eu

GITHUB_OWNER="${VIRTUES_GITHUB_OWNER:-virtues-os}"
GITHUB_REPO="${VIRTUES_GITHUB_REPO:-virtues}"
VIRTUES_VERSION="${VIRTUES_VERSION:-latest}"

# Brand mark. Bootstrap runs BEFORE the installer's ensure_utf8_locale step,
# but ssh forwards the client's LANG/LC_* (SendEnv on stock clients, AcceptEnv
# on Debian/Ubuntu sshd) — so the locale env here is evidence about the very
# terminal that will render this output. UTF-8 locale → show the real mark;
# otherwise (serial console, LANG=C, ancient setups) degrade to its ASCII
# spelling rather than print mojibake. Errors stay pure ASCII always.
case "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" in
    *[Uu][Tt][Ff]-8*|*[Uu][Tt][Ff]8*) MARK="∴" ;;
    *) MARK=":." ;;
esac

die() { printf "  x  %s\n" "$*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || die "must be run as root. Try: curl -sSL https://virtues.com/sh | sudo sh"

OS=$(uname -s); ARCH=$(uname -m)
[ "$OS" = "Linux" ] || die "Linux-only. Detected: $OS"
case "$ARCH" in
    x86_64|amd64) ARCH=x86_64 ;;
    aarch64|arm64) ARCH=aarch64 ;;
    *) die "unsupported arch: $ARCH (need x86_64 or aarch64)" ;;
esac

command -v curl >/dev/null 2>&1 || die "curl is required but not installed"

# Resolve "latest" via the GitHub Releases API — the newest stable *Linux* tag.
#
# This used to hit `/releases/latest`, which is wrong here for a reason that
# stayed hidden for months: ONE repo publishes both products. The Linux box ships
# as `vX.Y.Z` / `edge`, and the macOS app as `mac-vX.Y.Z` / `mac-latest`.
# `/releases/latest` returns the newest non-prerelease of ANY of them, so which
# product a Linux box installed depended on which had been released more
# recently. The Linux tags simply happened to be newer, so it worked.
#
# On 2026-08-17 the stable Linux releases were deleted to reset the version line,
# and `/releases/latest` immediately began answering `mac-latest` — meaning
# `curl virtues.com/sh | sudo sh` would fetch
# `.../download/mac-latest/virtues-installer-mac-latest-<arch>-linux` and 404.
# The Rust upgrader has filtered `mac-` tags for exactly this reason
# (`is_linux_tag`); the shell entrypoint, which is what actually installs a box,
# never learned to.
#
# Parsed with grep/sed/awk rather than jq: this runs on a bare Ubuntu box before
# anything of ours is installed, and requiring a JSON parser to bootstrap is how
# an installer fails on the one machine that matters. GitHub emits `tag_name`,
# `draft` and `prerelease` in that order per release, so `paste - - -` regroups
# them into one line each; releases come back newest-first, so the first match
# wins. Asset objects carry none of those three keys and cannot collide.
resolve_tag() {
    local api="https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}/releases?per_page=30"
    curl -sSLf -H "Accept: application/vnd.github+json" "$api" 2>/dev/null \
        | grep -oE '"tag_name":[[:space:]]*"[^"]*"|"(draft|prerelease)":[[:space:]]*(true|false)' \
        | sed -E 's/.*:[[:space:]]*"?([^"]*)"?$/\1/' \
        | paste - - - \
        | awk -F'\t' '$2=="false" && $3=="false" && $1 !~ /^mac-/ { print $1; exit }'
}

if [ "$VIRTUES_VERSION" = "latest" ]; then
    VIRTUES_VERSION=$(resolve_tag) \
        || die "could not resolve latest release. Pass VIRTUES_VERSION=vX.Y.Z to pin."
    # Empty means the API answered but no stable *Linux* release exists — a
    # different situation from the API being unreachable, and worth saying so.
    # It is the state right after a version-line reset, when only prereleases
    # are published.
    [ -n "$VIRTUES_VERSION" ] \
        || die "no stable Linux release is published yet.
       Install the prerelease channel:  curl -sSL https://virtues.com/sh-pre | sudo sh
       Or pin a tag:                    VIRTUES_VERSION=vX.Y.Z"
fi

# Hand the resolved tag down to the installer so it fetches the SAME release's
# tarball — not whatever its own `releases/latest` lookup would pick. This is
# what makes the pre channel work: the edge copy of this script defaults
# VIRTUES_VERSION to `edge` (stamped at release time, see release-linux.yml), so
# `curl virtues.com/sh-pre | sh` installs edge binaries instead of stable. The
# installer reads VIRTUES_VERSION from the env (see download.rs resolve_version).
export VIRTUES_VERSION

BASE="https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download/${VIRTUES_VERSION}"
NAME="virtues-installer-${VIRTUES_VERSION}-${ARCH}-linux"
INSTALLER_URL="${BASE}/${NAME}"
SHA_URL="${INSTALLER_URL}.sha256"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
INSTALLER="$TMPDIR/virtues-installer"

printf "  %s  Fetching installer (%s)...\n" "$MARK" "$VIRTUES_VERSION"
curl -sSLfo "$INSTALLER" "$INSTALLER_URL" \
    || die "download failed: $INSTALLER_URL"

# SHA256 verification. The sidecar is uploaded alongside the binary by CI, so
# a missing one is a packaging bug, not an optional nicety — and this binary is
# about to be executed as root. Hard-fail rather than run it unverified: a
# soft-fail here means anything that can suppress just the sidecar request (a
# captive portal, a CDN edge 404, an attacker serving a swapped binary while
# dropping the .sha256) silently downgrades this to "we trust HTTPS alone".
curl -sSLfo "$INSTALLER.sha256" "$SHA_URL" 2>/dev/null \
    || die "could not fetch checksum: $SHA_URL - refusing to execute unverified"

expected=$(awk '{print $1}' "$INSTALLER.sha256")
[ -n "$expected" ] || die "malformed checksum sidecar for $NAME"

# Verify the hasher exists before trusting its output — an absent sha256sum
# would yield an empty `actual` and turn the comparison below into a no-op.
if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$INSTALLER" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$INSTALLER" | awk '{print $1}')
else
    die "no sha256sum/shasum available to verify $NAME - refusing to execute"
fi

[ "$expected" = "$actual" ] \
    || die "SHA256 mismatch on $NAME - refusing to execute"

chmod +x "$INSTALLER"
exec "$INSTALLER" "$@"
