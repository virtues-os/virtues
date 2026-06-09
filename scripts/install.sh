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
DRY_RUN=0

# Flag parsing. `--dry-run` runs through every check and prints what would
# happen without actually modifying the system. `--version=X` pins a tag.
for arg in "$@"; do
    case "$arg" in
        --version=*) VIRTUES_VERSION="${arg#*=}" ;;
        --dry-run)   DRY_RUN=1 ;;
        --help|-h)
            cat <<'HELP'
Virtues installer.

Usage:
  curl -sSL https://get.virtues.com | sudo sh
  curl -sSL https://get.virtues.com | sudo sh -s -- [flags]

Flags:
  --version=vX.Y.Z   Pin a specific release (default: latest GitHub release)
  --dry-run          Print every step without modifying the system
  --help, -h         Show this help
HELP
            exit 0 ;;
        *) echo "unknown flag: $arg (try --help)" >&2; exit 1 ;;
    esac
done

# TTY-aware color: avoid ANSI gibberish in systemd / CI logs.
if [ -t 1 ]; then
    C_GREEN='\033[32m'; C_YELLOW='\033[33m'; C_RED='\033[31m'; C_DIM='\033[2m'; C_RESET='\033[0m'
else
    C_GREEN=''; C_YELLOW=''; C_RED=''; C_DIM=''; C_RESET=''
fi

say()    { printf "  %s\n"   "$*"; }
warn()   { printf "  ${C_YELLOW}⚠${C_RESET}  %s\n" "$*" >&2; }
die()    { printf "  ${C_RED}✖${C_RESET}  %s\n"  "$*" >&2; exit 1; }
header() { printf "\n  ${C_GREEN}%s${C_RESET}\n" "$*"; }
ok()     { printf "  ${C_GREEN}✓${C_RESET}  %s\n" "$*"; }

# In dry-run mode, every privileged or stateful command is gated through
# this helper so the user can preview the install without committing.
run() {
    if [ "$DRY_RUN" = "1" ]; then
        printf "  ${C_DIM}[dry-run]${C_RESET} %s\n" "$*"
    else
        "$@"
    fi
}

# Where verbose command output lands so the user-facing log stays tidy.
# Each `step` invocation appends; on failure we print the tail.
INSTALL_LOG="${INSTALL_LOG:-/tmp/virtues-install.log}"
: > "$INSTALL_LOG" 2>/dev/null || true

# Run a step quietly: animate a spinner while the command runs, redirect
# stdout+stderr to $INSTALL_LOG, and print ✓/✖ when done. On failure,
# tail the last 30 log lines so the user has context without scrolling
# 200 lines of apt output.
#
# Usage:
#     step "Installing Postgres 18" apt-get install -y -qq postgresql-18
#     step "Pulling embedding model" ollama pull bge-m3
#
# For pipelines or shell-redirected commands, wrap in `bash -c`:
#     step "Installing Ollama" bash -c 'curl -fsSL https://ollama.com/install.sh | sh'
step() {
    local title="$1"; shift
    if [ "$DRY_RUN" = "1" ]; then
        printf "  ${C_DIM}[dry-run]${C_RESET} %s\n" "$title"
        return 0
    fi

    # Without a TTY, fall back to a one-line "..." → ✓ pattern (no spinner
    # frames). Keeps journal / CI output readable.
    if [ ! -t 1 ]; then
        printf "  · %s ... " "$title"
        if "$@" >>"$INSTALL_LOG" 2>&1; then
            printf "ok\n"
            return 0
        else
            local code=$?
            printf "FAILED (exit %d)\n" "$code"
            printf "\n  Last log lines (full log: %s):\n" "$INSTALL_LOG" >&2
            tail -30 "$INSTALL_LOG" | sed 's/^/    /' >&2
            return $code
        fi
    fi

    # Interactive: animated spinner.
    local frames='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'
    printf "  ${C_DIM}⠋${C_RESET} %s" "$title"
    ( "$@" >>"$INSTALL_LOG" 2>&1 ) &
    local pid=$!
    local i=0
    while kill -0 "$pid" 2>/dev/null; do
        i=$(( (i + 1) % 10 ))
        printf "\r  ${C_DIM}%s${C_RESET} %s" "${frames:$i:1}" "$title"
        sleep 0.1
    done
    wait "$pid"
    local code=$?
    if [ "$code" -eq 0 ]; then
        printf "\r  ${C_GREEN}✓${C_RESET} %s\n" "$title"
        return 0
    else
        printf "\r  ${C_RED}✖${C_RESET} %s (exit %d)\n" "$title" "$code"
        printf "\n  Last log lines (full log: %s):\n" "$INSTALL_LOG" >&2
        tail -30 "$INSTALL_LOG" | sed 's/^/    /' >&2
        return $code
    fi
}

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
                # Ubuntu 22.04 LTS (jammy) is Jetson JetPack 6.x's base — supported via PGDG.
                22.04) USE_PGDG=1; say "Ubuntu 22.04 LTS detected (likely Jetson JetPack 6.x) — will add PGDG repo for Postgres 18." ;;
                24.04) USE_PGDG=1; say "Ubuntu 24.04 LTS detected — will add PGDG repo for Postgres 18." ;;
                25.04|25.10) USE_PGDG=1; say "Ubuntu $DISTRO_VERSION detected — will add PGDG repo for Postgres 18." ;;
                26.04|26.10|2[6-9].*|[3-9][0-9].*) : ;;  # ships PG18 natively
                *) die "Ubuntu $DISTRO_VERSION is not supported. Virtues v1 requires Ubuntu 22.04 LTS or later." ;;
            esac
            ;;
        fedora)
            if [ "$DISTRO_VERSION" -lt 40 ] 2>/dev/null; then
                die "Fedora $DISTRO_VERSION is not supported. Virtues v1 requires Fedora 40 or later."
            fi
            ;;
    esac

    # No glibc gate any more — v0.1.0 routes all local ML through Ollama
    # (separate daemon, see `ensure_ollama` below). The virtues binary
    # itself only needs glibc 2.31+ (covered by every supported distro).
}

# Pre-flight: fail fast on environmental problems BEFORE we start touching
# apt/systemd/PG/Ollama. Catches the failures that produce ugly half-installs
# (out of disk mid-Ollama-pull, network blocked, port already in use, etc.).
preflight_checks() {
    header "🩺  Pre-flight checks…"
    local issues=0

    # Disk space: 3GB minimum for PG18 + virtues binary + bge-m3 model.
    local free_kb
    free_kb="$(df -kP / | awk 'NR==2 {print $4}')"
    if [ "$free_kb" -lt 3145728 ]; then
        warn "Free disk space on / is $(( free_kb / 1024 )) MB — recommend ≥ 3 GB."
        issues=$((issues + 1))
    else
        ok "Free disk space on /: $(( free_kb / 1024 / 1024 )) GB"
    fi

    # Network reachability for the three hosts we depend on.
    local host
    for host in github.com ollama.com apt.postgresql.org; do
        if curl -fsS --max-time 5 -o /dev/null --head "https://$host"; then
            ok "Reachable: https://$host"
        else
            warn "Cannot reach https://$host — install may fail mid-way."
            issues=$((issues + 1))
        fi
    done

    # Downloader detection. We use curl elsewhere; warn if missing (unusual).
    if ! command -v curl >/dev/null 2>&1; then
        warn "curl is not installed — required for binary download."
        issues=$((issues + 1))
    fi

    # Port conflicts: 5432 (postgres), 8000 (virtues HTTP), 11434 (Ollama).
    local port name
    for entry in "5432:postgres" "8000:virtues" "11434:ollama"; do
        port="${entry%%:*}"; name="${entry#*:}"
        if (echo > "/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
            warn "Port $port (${name}) is already in use by another process."
            issues=$((issues + 1))
        fi
    done

    if [ "$issues" -gt 0 ]; then
        if [ "$DRY_RUN" = "1" ]; then
            warn "$issues pre-flight issue(s). Re-run without --dry-run when ready."
        else
            warn "$issues pre-flight issue(s) detected — continuing in 5s (Ctrl+C to abort)…"
            sleep 5
        fi
    else
        ok "All pre-flight checks passed."
    fi
}

add_pgdg_repo() {
    [ "$USE_PGDG" = "1" ] || return 0
    header "🔧  Adding PGDG repo (Postgres 18 isn't in your distro's default repos)"
    step "Installing apt key tooling (curl, lsb-release, gnupg)" \
        apt-get install -y -qq curl ca-certificates lsb-release gnupg
    install -d /usr/share/postgresql-common/pgdg
    step "Fetching PGDG signing key" \
        curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
        -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc
    local codename
    codename="$(lsb_release -cs)"
    echo "deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] https://apt.postgresql.org/pub/repos/apt ${codename}-pgdg main" \
        > /etc/apt/sources.list.d/pgdg.list
    step "Refreshing apt index with PGDG repo" apt-get update -qq
}

install_deps() {
    header "📦  Installing system dependencies"
    case "$PKG" in
        apt)
            export DEBIAN_FRONTEND=noninteractive
            step "Refreshing apt index" apt-get update -qq
            add_pgdg_repo
            step "Installing Postgres 18 + pgvector" \
                apt-get install -y -qq postgresql-18 postgresql-18-pgvector
            step "Installing WireGuard" \
                apt-get install -y -qq wireguard wireguard-tools
            step "Installing Avahi (mDNS)" \
                apt-get install -y -qq avahi-daemon avahi-utils libnss-mdns
            step "Installing ca-certificates + curl" \
                apt-get install -y -qq ca-certificates curl
            ;;
        dnf)
            step "Installing Postgres + pgvector" \
                dnf install -y -q postgresql-server postgresql-contrib pgvector
            step "Installing WireGuard tooling" \
                dnf install -y -q wireguard-tools
            step "Installing Avahi (mDNS)" \
                dnf install -y -q avahi nss-mdns
            step "Installing ca-certificates + curl" \
                dnf install -y -q ca-certificates curl
            # Fedora's postgresql-setup --initdb is required before first start.
            if [ ! -d /var/lib/pgsql/data/base ]; then
                step "Initializing Fedora Postgres cluster" \
                    postgresql-setup --initdb
            fi
            ;;
    esac
    step "Enabling postgresql service" systemctl enable --now postgresql
    step "Enabling avahi-daemon service" systemctl enable --now avahi-daemon
}

# Ensure Ollama is installed + running. Ollama owns local embeddings (and
# eventually reranking + on-box chat) in v0.1.0; the virtues binary calls
# its HTTP API at localhost:11434. The official installer detects GPU/CPU
# and configures the systemd unit; if it's already installed we no-op.
ensure_ollama() {
    header "🦙  Installing Ollama (local inference daemon)"
    if command -v ollama >/dev/null 2>&1; then
        ok "Ollama already installed: $(ollama --version 2>/dev/null | head -1)"
    else
        # Official install script — detects glibc, CUDA, ROCm, arch, etc.
        # Wrapped in bash -c so we can pipe-then-exec without killing
        # the outer shell on set -e.
        step "Installing Ollama daemon" \
            bash -c 'curl -fsSL https://ollama.com/install.sh | sh' \
            || die "Ollama install failed. Install manually from https://ollama.com and re-run."
        ok "Ollama installed: $(ollama --version 2>/dev/null | head -1)"
    fi
    step "Enabling ollama service" \
        bash -c 'systemctl enable --now ollama 2>/dev/null || true'

    # Pull the default embedding model so the first query doesn't pay
    # download latency. Operators can swap the model via VIRTUES_EMBED_MODEL.
    EMBED_MODEL="${VIRTUES_EMBED_MODEL:-bge-m3}"
    step "Pulling embedding model: $EMBED_MODEL (~1.2 GB, one-time)" \
        ollama pull "$EMBED_MODEL" \
        || warn "ollama pull $EMBED_MODEL failed; first embed request will retry."
}

# Make this box discoverable on the LAN as `virtues.local`.
#
# Two pieces:
#   1. Hostname — set to "virtues" so the kernel and Avahi advertise it
#      (any LAN-resolvable mDNS name comes from `$hostname.local`).
#   2. Service advertisement — drop an Avahi service-group file so the
#      box appears in Bonjour Browser / `dns-sd -B _https._tcp` listings.
configure_mdns() {
    header "📡  Configuring mDNS (virtues.local on the LAN)"

    local current_host
    current_host=$(hostnamectl --static 2>/dev/null || hostname)
    if [ "$current_host" = "virtues" ]; then
        ok "Hostname already 'virtues'"
    else
        if [ "${VIRTUES_KEEP_HOSTNAME:-0}" = "1" ]; then
            warn "Keeping existing hostname '$current_host' (VIRTUES_KEEP_HOSTNAME=1)."
            warn "Box will be reachable at https://${current_host}.local, not virtues.local."
        else
            step "Setting hostname → 'virtues' (was '$current_host')" \
                hostnamectl set-hostname virtues
        fi
    fi

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
    step "Advertising _https._tcp via avahi-daemon" \
        bash -c 'systemctl reload avahi-daemon 2>/dev/null || systemctl restart avahi-daemon'
}

create_user() {
    header "👤  Creating 'virtues' system user"
    if ! id -u virtues >/dev/null 2>&1; then
        step "Creating system user 'virtues'" \
            useradd --system --home-dir "$DATA_DIR" --shell /usr/sbin/nologin virtues
    else
        ok "User 'virtues' already exists"
    fi
    step "Setting up $DATA_DIR (lake, models, secrets)" \
        bash -c "mkdir -p '$DATA_DIR/lake' '$DATA_DIR/models' '$DATA_DIR/secrets' \
                 && chown -R virtues:virtues '$DATA_DIR' \
                 && chmod 0700 '$DATA_DIR/secrets'"
}

provision_db() {
    header "🗄   Provisioning Postgres role + database"
    if sudo -u postgres psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='virtues'" 2>/dev/null | grep -q 1; then
        ok "Postgres role 'virtues' already exists"
    else
        step "Creating Postgres role 'virtues'" \
            sudo -u postgres createuser --no-superuser --no-createrole --createdb virtues
    fi
    if sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='virtues'" 2>/dev/null | grep -q 1; then
        ok "Postgres database 'virtues' already exists"
    else
        step "Creating Postgres database 'virtues'" \
            sudo -u postgres createdb -O virtues virtues
    fi
    # pgvector's CREATE EXTENSION requires superuser. Install it now as
    # 'postgres' so migrations (run later as the 'virtues' role) can use
    # the type without elevation.
    step "Installing pgvector extension into virtues DB" \
        sudo -u postgres psql -d virtues -c "CREATE EXTENSION IF NOT EXISTS vector"
}

# Run virtues bringup: applies migrations + ensures box identity.
# Idempotent — safe to re-run on every install.sh invocation.
run_bringup() {
    header "🚀  First-boot bringup (migrations + box identity)"
    step "Loading env + running virtues bringup" \
        sudo -u virtues bash -c "set -a; . '$DATA_DIR/virtues.env'; set +a; '$INSTALL_PREFIX/bin/virtues' bringup"
}

# Write the env file the systemd unit + CLI both read. DATABASE_URL uses
# the local Unix socket (peer auth, no password), and the encryption key
# is freshly generated on first run. We *only* generate the key if the
# file doesn't already exist — re-running install.sh on a working box
# must never rotate the key (would invalidate every stored credential).
write_env_file() {
    header "🔑  Writing /var/lib/virtues/virtues.env"
    local env_file="${DATA_DIR}/virtues.env"
    if [ -f "$env_file" ]; then
        ok "$env_file already exists — leaving in place"
        return 0
    fi
    local enc_key
    enc_key="$(openssl rand -base64 32)"
    cat > "$env_file" <<EOF
# Generated by install.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ).
# DATABASE_URL omits host -> Unix socket -> peer auth, no password.
DATABASE_URL=postgres:///virtues
VIRTUES_ENCRYPTION_KEY=$enc_key
ENVIRONMENT=production
EOF
    chown virtues:virtues "$env_file"
    chmod 0600 "$env_file"
    ok "Wrote $env_file"
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

    header "⬇   Downloading virtues binary ($VIRTUES_VERSION, $PLAT_ARCH-linux)"
    local tarball="virtues-${VIRTUES_VERSION}-${PLAT_ARCH}-linux.tar.gz"
    local url="${base}/${tarball}"
    local tmpdir
    tmpdir=$(mktemp -d)
    # Keep the outer EXIT trap (cleanup_on_exit); add our own tmpdir trap
    # by stacking the cleanup into the global handler instead of overwriting.
    VIRTUES_TMPDIR="$tmpdir"

    step "Downloading $tarball" curl -sSLfo "$tmpdir/$tarball" "$url"

    # Verify SHA256 if the .sha256 sidecar is available.
    if curl -sSLfo "$tmpdir/${tarball}.sha256" "${url}.sha256" 2>/dev/null; then
        local expected actual
        expected=$(awk '{print $1}' "$tmpdir/${tarball}.sha256")
        actual=$(sha256sum "$tmpdir/$tarball" | awk '{print $1}')
        if [ "$expected" = "$actual" ]; then
            ok "SHA256 verified"
        else
            die "sha256 mismatch on $tarball — refusing to install"
        fi
    fi

    step "Extracting + installing to $INSTALL_PREFIX/bin/virtues" \
        bash -c "tar -xzf '$tmpdir/$tarball' -C '$tmpdir' \
                 && install -m 0755 '$tmpdir/virtues' '$INSTALL_PREFIX/bin/virtues'"
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

# Post-install: sanity-check the things install.sh just set up so problems
# surface here (clear context) instead of later via a broken login URL.
post_install_health() {
    header "🩺  Post-install health check…"
    local issues=0

    # Postgres reachable as the virtues user? Use peer auth via the local
    # socket — no -h so psql doesn't try TCP password auth.
    if sudo -u virtues PGPASSWORD='' psql -d virtues -c 'SELECT 1' >/dev/null 2>&1 </dev/null; then
        ok "Postgres reachable as 'virtues' (peer auth)"
    else
        warn "Postgres connection as 'virtues' failed."
        issues=$((issues + 1))
    fi

    # Ollama daemon responding?
    if curl -fsS --max-time 5 -o /dev/null http://localhost:11434/api/tags; then
        ok "Ollama daemon responding on :11434"
    else
        warn "Ollama daemon not responding — start with: systemctl start ollama"
        issues=$((issues + 1))
    fi

    # Embedding model pulled?
    local embed_model="${VIRTUES_EMBED_MODEL:-bge-m3}"
    if command -v ollama >/dev/null 2>&1 && ollama list 2>/dev/null | grep -q "$embed_model"; then
        ok "Embedding model present: $embed_model"
    else
        warn "Embedding model not pulled — first embed call will retry: ollama pull $embed_model"
        issues=$((issues + 1))
    fi

    # virtues binary executable + reports its version?
    if "$INSTALL_PREFIX/bin/virtues" --version >/dev/null 2>&1; then
        ok "virtues binary OK: $("$INSTALL_PREFIX/bin/virtues" --version 2>&1 | head -1)"
    else
        warn "virtues binary failed --version probe; check $INSTALL_PREFIX/bin/virtues"
        issues=$((issues + 1))
    fi

    if [ "$issues" -gt 0 ]; then
        warn "$issues post-install issue(s). Run 'sudo -u virtues virtues doctor' for details."
    else
        ok "All post-install checks passed."
    fi
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

# Cleanup transient state on failure (tarballs, temp downloads). We
# deliberately DON'T try to undo systemd/apt/PG work — too risky.
cleanup_on_exit() {
    local code=$?
    rm -f /tmp/virtues-*.tar.gz /tmp/virtues-*.sha256 2>/dev/null || true
    if [ "$code" -ne 0 ] && [ "$DRY_RUN" = "0" ]; then
        printf "\n  ${C_RED}Install failed at step:${C_RESET} %s\n" "${FAILED_STEP:-unknown}" >&2
        printf "  ${C_DIM}Re-running this script is safe — completed steps are idempotent.${C_RESET}\n" >&2
        printf "  ${C_DIM}For help, see https://github.com/virtues-os/virtues/issues${C_RESET}\n" >&2
    fi
    return $code
}
trap cleanup_on_exit EXIT
trap 'send_install_beacon failed "$FAILED_STEP"' ERR

preflight_checks

# Short-circuit dry-run after pre-flight: the rest of the script would
# require apt/systemd state to make sense.
if [ "$DRY_RUN" = "1" ]; then
    header "Dry-run complete — no changes made."
    say "Pre-flight finished. Re-run without --dry-run to install."
    exit 0
fi

FAILED_STEP=install_deps;        install_deps
FAILED_STEP=ensure_ollama;       ensure_ollama
FAILED_STEP=configure_mdns;      configure_mdns
FAILED_STEP=create_user;         create_user
FAILED_STEP=provision_db;        provision_db
FAILED_STEP=download_binary;     download_binary
FAILED_STEP=write_env_file;      write_env_file
FAILED_STEP=run_bringup;         run_bringup
FAILED_STEP=install_systemd_unit; install_systemd_unit
FAILED_STEP=post_install_health; post_install_health
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
