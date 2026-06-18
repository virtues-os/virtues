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

# Resolve "latest" via the GitHub Releases API.
resolve_tag() {
    local api="https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}/releases/latest"
    curl -sSLf -H "Accept: application/vnd.github+json" "$api" 2>/dev/null \
        | grep -o '"tag_name":[[:space:]]*"[^"]*"' \
        | head -1 \
        | sed -E 's/.*"tag_name":[[:space:]]*"([^"]*)"/\1/'
}

if [ "$VIRTUES_VERSION" = "latest" ]; then
    VIRTUES_VERSION=$(resolve_tag) \
        || die "could not resolve latest release. Pass VIRTUES_VERSION=vX.Y.Z to pin."
    [ -n "$VIRTUES_VERSION" ] \
        || die "could not resolve latest release. Pass VIRTUES_VERSION=vX.Y.Z to pin."
fi

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

# SHA256 verification. Sidecar is uploaded alongside the binary by CI.
# Missing sidecar warns but continues — HTTPS is the primary trust layer.
if curl -sSLfo "$INSTALLER.sha256" "$SHA_URL" 2>/dev/null; then
    expected=$(awk '{print $1}' "$INSTALLER.sha256")
    actual=$(sha256sum "$INSTALLER" | awk '{print $1}')
    [ "$expected" = "$actual" ] \
        || die "SHA256 mismatch on $NAME - refusing to execute"
fi

chmod +x "$INSTALLER"
exec "$INSTALLER" "$@"
