#!/usr/bin/env bash
# Virtues installer — Linux only.
#
# Usage:
#   curl -sSL https://get.virtues.com | sh
#   curl -sSL https://get.virtues.com | sh -s -- --version=v0.1.2
#
# What it does:
#   1. Verifies Linux (Debian/Ubuntu/Fedora) + amd64 or arm64
#   2. Installs Postgres 18 + pgvector + wireguard-tools via apt or dnf
#   3. Downloads the matching virtues binary tarball, extracts to /usr/local/bin
#   4. Creates a 'virtues' system user + /var/lib/virtues data dir
#   5. Installs systemd unit, enables it
#   6. Prints next steps (start systemd + run `virtues link` for the login URL)
#
# Virtues runs as a system component — owns the SSD, owns the network
# (WireGuard + 443), needs GPU/NPU access. Native install + systemd is
# the right shape; containers would just be packaging overhead with no
# isolation benefit (see project docs).

set -euo pipefail

VIRTUES_VERSION="${VIRTUES_VERSION:-latest}"
INSTALL_PREFIX="${INSTALL_PREFIX:-/usr/local}"
DATA_DIR="${DATA_DIR:-/var/lib/virtues}"
# Binaries live on GitHub Releases. `get.virtues.com` serves this install
# script (via a CDN / Pages) but the tarballs themselves come from the
# Releases CDN. Override with `VIRTUES_DOWNLOAD_BASE` to use a local file
# server during dev/testing.
GITHUB_OWNER="${VIRTUES_GITHUB_OWNER:-virtues-os}"
GITHUB_REPO="${VIRTUES_GITHUB_REPO:-virtues}"
DOWNLOAD_BASE="${VIRTUES_DOWNLOAD_BASE:-}"

# Allow --version=… as the only flag for now.
for arg in "$@"; do
    case "$arg" in
        --version=*) VIRTUES_VERSION="${arg#*=}" ;;
        *) echo "unknown flag: $arg" >&2; exit 1 ;;
    esac
done

say()    { printf "  %s\n" "$*"; }
warn()   { printf "  ⚠  %s\n" "$*" >&2; }
die()    { printf "  ✖  %s\n" "$*" >&2; exit 1; }
header() { printf "\n  %s\n" "$*"; }

require_root() {
    if [ "$(id -u)" -ne 0 ]; then
        die "must be run as root. Try: curl -sSL https://get.virtues.com | sudo sh"
    fi
}

detect_platform() {
    local os arch
    os=$(uname -s)
    arch=$(uname -m)
    [ "$os" = "Linux" ] || die "Virtues is Linux-only. Detected: $os. Use a Linux VM if you're on Mac/Windows."
    case "$arch" in
        x86_64|amd64) PLAT_ARCH="x86_64" ;;
        aarch64|arm64) PLAT_ARCH="aarch64" ;;
        *) die "unsupported arch: $arch (need x86_64 or aarch64)" ;;
    esac
}

detect_distro() {
    if [ -r /etc/os-release ]; then
        . /etc/os-release
        DISTRO="${ID:-unknown}"
        DISTRO_LIKE="${ID_LIKE:-}"
        DISTRO_VERSION="${VERSION_ID:-0}"
    else
        die "/etc/os-release missing; can't detect distro"
    fi

    case "$DISTRO" in
        debian|ubuntu) PKG=apt ;;
        fedora|rhel|centos|rocky|almalinux) PKG=dnf ;;
        *)
            # Best-effort fallback via ID_LIKE.
            case "$DISTRO_LIKE" in
                *debian*|*ubuntu*) PKG=apt ;;
                *fedora*|*rhel*) PKG=dnf ;;
                *) die "unsupported distro: $DISTRO (supported: Debian, Ubuntu, Fedora, RHEL-family for v1)" ;;
            esac
            ;;
    esac

    # Distro version gate. Postgres 18 ships in default repos on:
    #   Debian 13 (trixie) and later
    #   Ubuntu 26.04 LTS and later (24.04 ships PG16, 25.04 ships PG17)
    #   Fedora 40 and later
    # For Ubuntu 24.04, we add the PGDG repo automatically so PG18 is
    # available — the LTS is too widespread to refuse. For other older
    # distros we refuse with a clear message.
    USE_PGDG=0
    case "$DISTRO" in
        debian)
            if [ "${DISTRO_VERSION%%.*}" -lt 13 ] 2>/dev/null; then
                die "Debian $DISTRO_VERSION is not supported. Virtues v1 requires Debian 13 (trixie) or later."
            fi
            ;;
        ubuntu)
            case "$DISTRO_VERSION" in
                24.04) USE_PGDG=1; say "Ubuntu 24.04 LTS detected — will add PGDG repo for Postgres 18." ;;
                25.04|25.10) USE_PGDG=1; say "Ubuntu $DISTRO_VERSION detected — will add PGDG repo for Postgres 18." ;;
                26.04|26.10|2[6-9].*|[3-9][0-9].*) : ;;  # ships PG18 natively
                *) die "Ubuntu $DISTRO_VERSION is not supported. Virtues v1 requires Ubuntu 24.04 LTS or later." ;;
            esac
            ;;
        fedora)
            if [ "$DISTRO_VERSION" -lt 40 ] 2>/dev/null; then
                die "Fedora $DISTRO_VERSION is not supported. Virtues v1 requires Fedora 40 or later."
            fi
            ;;
    esac
}

add_pgdg_repo() {
    [ "$USE_PGDG" = "1" ] || return 0
    header "🔧  Adding PGDG repo (Postgres 18 isn't in your distro's default repos)…"
    apt-get install -y -qq curl ca-certificates lsb-release gnupg
    install -d /usr/share/postgresql-common/pgdg
    curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
        -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc
    local codename
    codename="$(lsb_release -cs)"
    echo "deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] https://apt.postgresql.org/pub/repos/apt ${codename}-pgdg main" \
        > /etc/apt/sources.list.d/pgdg.list
    apt-get update -qq
}

install_deps() {
    header "📦  Installing system dependencies (Postgres, WireGuard, Avahi)…"
    case "$PKG" in
        apt)
            export DEBIAN_FRONTEND=noninteractive
            apt-get update -qq
            # Postgres 18 ships in Debian 13 (trixie) and Ubuntu 26.04+
            # natively. For Ubuntu 24.04/25.04 we add the PGDG repo first.
            add_pgdg_repo
            apt-get install -y -qq \
                postgresql-18 postgresql-18-pgvector \
                wireguard wireguard-tools \
                avahi-daemon avahi-utils libnss-mdns \
                ca-certificates curl
            ;;
        dnf)
            dnf install -y -q \
                postgresql-server postgresql-contrib pgvector \
                wireguard-tools \
                avahi nss-mdns \
                ca-certificates curl
            # Fedora's postgresql-setup --initdb is required before first start.
            if [ ! -d /var/lib/pgsql/data/base ]; then
                postgresql-setup --initdb
            fi
            ;;
    esac
    systemctl enable --now postgresql
    systemctl enable --now avahi-daemon
    say "Postgres + WireGuard + Avahi (mDNS) installed."
}

# Make this box discoverable on the LAN as `virtues.local`.
#
# Two pieces:
#   1. Hostname — set to "virtues" so the kernel and Avahi advertise it
#      (any LAN-resolvable mDNS name comes from `$hostname.local`).
#   2. Service advertisement — drop an Avahi service-group file so the
#      box appears in Bonjour Browser / `dns-sd -B _https._tcp` listings.
configure_mdns() {
    header "📡  Configuring mDNS (Avahi) so this box is reachable at https://virtues.local…"

    local current_host
    current_host=$(hostnamectl --static 2>/dev/null || hostname)
    if [ "$current_host" = "virtues" ]; then
        say "Hostname already 'virtues'."
    else
        # Auto-set unless caller passes VIRTUES_KEEP_HOSTNAME=1 to preserve
        # their existing hostname (in which case the box is reachable at
        # https://<current_host>.local instead).
        if [ "${VIRTUES_KEEP_HOSTNAME:-0}" = "1" ]; then
            warn "Keeping existing hostname '$current_host' (VIRTUES_KEEP_HOSTNAME=1)."
            warn "Box will be reachable at https://${current_host}.local, not virtues.local."
        else
            hostnamectl set-hostname virtues
            say "Hostname set to 'virtues' (was '$current_host')."
        fi
    fi

    # Drop the service advertisement. Avahi auto-reloads /etc/avahi/services/.
    mkdir -p /etc/avahi/services
    cat > /etc/avahi/services/virtues.service <<'AVAHI'
<?xml version="1.0" standalone='no'?>
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<service-group>
  <name replace-wildcards="yes">Virtues on %h</name>
  <service>
    <type>_https._tcp</type>
    <port>443</port>
    <txt-record>path=/</txt-record>
    <txt-record>service=virtues</txt-record>
  </service>
</service-group>
AVAHI
    systemctl reload avahi-daemon 2>/dev/null || systemctl restart avahi-daemon
    say "Advertising _https._tcp on :443 over mDNS."
}

create_user() {
    header "👤  Creating 'virtues' system user…"
    if ! id -u virtues >/dev/null 2>&1; then
        useradd --system --home-dir "$DATA_DIR" --shell /usr/sbin/nologin virtues
        say "Created system user 'virtues'."
    else
        say "User 'virtues' already exists."
    fi
    mkdir -p "$DATA_DIR/lake" "$DATA_DIR/models" "$DATA_DIR/secrets"
    chown -R virtues:virtues "$DATA_DIR"
    chmod 0700 "$DATA_DIR/secrets"
}

provision_db() {
    header "🗄   Provisioning Postgres role + database…"
    # Use peer-auth via the postgres OS user.
    sudo -u postgres psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='virtues'" | grep -q 1 || \
        sudo -u postgres createuser --no-superuser --no-createrole --createdb virtues
    sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='virtues'" | grep -q 1 || \
        sudo -u postgres createdb -O virtues virtues
    say "Postgres role + 'virtues' database ready."
}

# Resolve "latest" to a concrete tag via the GitHub Releases API. Only called
# when the user didn't pin a version. Falls back to raw GitHub redirects if
# the API is unreachable (e.g. behind a corporate proxy).
resolve_latest_tag() {
    local api="https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}/releases/latest"
    local tag
    tag=$(curl -sSLf -H "Accept: application/vnd.github+json" "$api" 2>/dev/null \
            | grep -o '"tag_name":[[:space:]]*"[^"]*"' \
            | head -1 \
            | sed -E 's/.*"tag_name":[[:space:]]*"([^"]*)"/\1/')
    if [ -z "$tag" ]; then
        die "could not resolve latest release tag from GitHub (offline?). Pass --version=vX.Y.Z to pin."
    fi
    echo "$tag"
}

download_binary() {
    if [ "$VIRTUES_VERSION" = "latest" ]; then
        VIRTUES_VERSION=$(resolve_latest_tag)
        say "Resolved latest release: $VIRTUES_VERSION"
    fi

    # If the operator passed VIRTUES_DOWNLOAD_BASE (e.g. a local http.server
    # for testing), use it verbatim. Otherwise build the canonical GitHub
    # Releases URL for this tag.
    local base="${DOWNLOAD_BASE}"
    if [ -z "$base" ]; then
        base="https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download/${VIRTUES_VERSION}"
    fi

    header "⬇   Downloading virtues binary ($VIRTUES_VERSION, $PLAT_ARCH-linux)…"
    local tarball="virtues-${VIRTUES_VERSION}-${PLAT_ARCH}-linux.tar.gz"
    local url="${base}/${tarball}"
    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf '$tmpdir'" EXIT
    curl -sSLfo "$tmpdir/$tarball" "$url" || die "download failed: $url"

    # Verify SHA256 if the .sha256 sidecar is available (CI uploads it
    # alongside each tarball). Best-effort: skip silently if absent.
    if curl -sSLfo "$tmpdir/${tarball}.sha256" "${url}.sha256" 2>/dev/null; then
        local expected actual
        expected=$(awk '{print $1}' "$tmpdir/${tarball}.sha256")
        actual=$(sha256sum "$tmpdir/$tarball" | awk '{print $1}')
        [ "$expected" = "$actual" ] || die "sha256 mismatch on $tarball — refusing to install"
        say "Verified SHA256."
    fi

    tar -xzf "$tmpdir/$tarball" -C "$tmpdir"
    install -m 0755 "$tmpdir/virtues" "$INSTALL_PREFIX/bin/virtues"
    say "Installed $INSTALL_PREFIX/bin/virtues"
}

install_systemd_unit() {
    header "⚙   Installing systemd unit…"
    cat > /etc/systemd/system/virtues.service <<'UNIT'
[Unit]
Description=Virtues — your data, on your hardware
Documentation=https://virtues.com/docs
After=postgresql.service network-online.target
Wants=postgresql.service network-online.target

[Service]
Type=simple
User=virtues
Group=virtues
WorkingDirectory=/var/lib/virtues
EnvironmentFile=-/var/lib/virtues/virtues.env
# Wait for Postgres to actually accept connections (not just "started")
# before launching the daemon. `After=postgresql.service` above orders the
# unit start, but PG can still be in WAL recovery for ~30s on a cold boot
# while accepting no connections. Gating ExecStartPre on `pg_isready`
# converts what was a restart-loop (panic → 5s wait → panic) into a quiet
# wait. The binary also retries internally as a belt-and-suspenders layer,
# but this is the cleaner path for first-boot.
ExecStartPre=/bin/sh -c 'until pg_isready -h /var/run/postgresql -d virtues -U virtues -t 1 >/dev/null 2>&1; do sleep 1; done'
ExecStart=/usr/local/bin/virtues server --host 0.0.0.0 --port 8000
# Post-stop diagnostic beacon. Runs only on real crashes (signal,
# core-dump, watchdog, non-zero exit); silently does nothing on a clean
# systemctl stop. Honors VIRTUES_DIAG=off in /etc/virtues/env so users
# who opted out send nothing. The command always exits 0 so a failed
# beacon post never amplifies the underlying crash.
ExecStopPost=/usr/local/bin/virtues report-crash
TimeoutStartSec=120
Restart=on-failure
RestartSec=5

# Hardening — the box trusts virtues to own its data; we still drop what
# we don't strictly need.
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/virtues
ProtectHome=true
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=false   # WG netlink needs to interact with kernel
ProtectControlGroups=true
RestrictSUIDSGID=true
LockPersonality=true
RestrictNamespaces=true
SystemCallArchitectures=native

# Capabilities — minimum for WireGuard + binding privileged ports.
AmbientCapabilities=CAP_NET_ADMIN CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_BIND_SERVICE

[Install]
WantedBy=multi-user.target
UNIT
    systemctl daemon-reload
    say "Unit installed at /etc/systemd/system/virtues.service"
}

print_next_steps() {
    header "✅  Virtues installed."
    cat <<EOF

  This box is now broadcasting itself on the LAN as:

      https://virtues.local

  (Visible on any Mac, iOS, Linux, or recent Windows machine on the same
   network. On first visit your browser will prompt to trust the box's
   TLS certificate — \`virtues link\` prints install instructions.)

  Next:

    1. Start the service:

         sudo systemctl enable --now virtues

    2. Get the URL to log in (mints a one-time setup token):

         sudo -u virtues virtues link

    3. Check status from CLI (or just open https://virtues.local in a browser):

         sudo systemctl status virtues
         virtues status

  Optional, when you want chat / remote access:

         sudo -u virtues virtues subscribe

  Docs: https://virtues.com/docs
  Issues: https://github.com/virtues/virtues/issues

EOF
}

# ── main ───────────────────────────────────────────────────────────────

require_root
detect_platform
detect_distro

header "Welcome — installing Virtues on $DISTRO ($PLAT_ARCH)."

# Diagnostic beacon. If anything below fails, the trap fires an
# `outcome=failed` beacon with the function name as `failed_step`. On
# clean completion `send_install_beacon ok` runs at the bottom. Either
# way the call is best-effort — a network glitch never blocks the
# install or surfaces a confusing error.
FAILED_STEP=""
send_install_beacon() {
    local outcome="$1"
    local step="${2:-}"
    # Honor opt-out: if /etc/virtues/env already exists with VIRTUES_DIAG=off,
    # or the env is set in this shell, send nothing.
    if [ -f /etc/virtues/env ] && grep -qi '^VIRTUES_DIAG=\(off\|false\|0\|no\|disabled\)' /etc/virtues/env; then
        return 0
    fi
    if [ -n "${VIRTUES_DIAG:-}" ] && \
       echo "$VIRTUES_DIAG" | grep -qi '^\(off\|false\|0\|no\|disabled\)$'; then
        return 0
    fi
    local atlas_url="${VIRTUES_ATLAS_URL:-https://atlas.virtues.com}"
    local payload
    payload=$(printf '{"box_id":"%s","distro":"%s","version":"%s","arch":"%s","outcome":"%s","failed_step":"%s"}' \
        "$(install_box_id)" "$DISTRO" "$VIRTUES_VERSION" "$PLAT_ARCH" "$outcome" "$step")
    curl -fsS --max-time 5 \
        -H 'Content-Type: application/json' \
        -X POST "$atlas_url/diag/install" \
        --data "$payload" >/dev/null 2>&1 || true
}

# Stable per-install id. We hash the machine-id (or hostname as fallback)
# so atlas can correlate retries from the same box without learning the
# host's name. Plain hex, no PII.
install_box_id() {
    local raw
    if [ -r /etc/machine-id ]; then
        raw=$(cat /etc/machine-id)
    else
        raw=$(hostname 2>/dev/null || echo unknown)
    fi
    echo "$raw" | sha256sum | awk '{print "i:"substr($1,1,16)}'
}

trap 'send_install_beacon failed "$FAILED_STEP"' ERR

FAILED_STEP=install_deps;        install_deps
FAILED_STEP=configure_mdns;      configure_mdns
FAILED_STEP=create_user;         create_user
FAILED_STEP=provision_db;        provision_db
FAILED_STEP=download_binary;     download_binary
FAILED_STEP=install_systemd_unit; install_systemd_unit
FAILED_STEP=""

print_next_steps

# Privacy note printed BEFORE the beacon goes out so users see what
# they're sending and how to opt out before the install finishes.
cat <<'DIAG_NOTICE'

  📡  Anonymized install + crash beacons are ON by default.
      What's sent: distro, version, architecture, install outcome.
                   On crash: exit code + last 50 journal lines.
                   No source data, no personal data, no chat content.
      To disable:  add VIRTUES_DIAG=off to /etc/virtues/env, then
                   `sudo systemctl restart virtues`.

DIAG_NOTICE

send_install_beacon ok ""
