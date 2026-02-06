#!/bin/bash
# VPS Bootstrap Script for Virtues Tenant
# Nomad + containerd + gVisor + Traefik Stack
#
# Called by cloud-init during Hetzner VPS provisioning
#
# Expected environment (passed via cloud-init .env):
# - SUBDOMAIN (required)
# - TIER (required: 'free', 'standard', or 'pro')
# - OWNER_EMAIL (required)
# - VIRTUES_ENCRYPTION_KEY (required)
# - GHCR_REPO (required: GitHub Container Registry repository)
# - HETZNER_DNS_API_TOKEN (required for wildcard SSL)
# - HETZNER_STORAGE_BOX_USER (optional, for Infinite Drive)
# - HETZNER_STORAGE_BOX_PASSWORD (optional, for Infinite Drive)

set -euo pipefail

# ============================================================================
# Logging and Error Handling
# ============================================================================

LOG_FILE="/var/log/virtues-setup.log"

log() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] $*" | tee -a "$LOG_FILE"
}

error_exit() {
    log "ERROR: $1"
    cleanup_on_error
    exit 1
}

cleanup_on_error() {
    log "Cleaning up after error..."
    nomad job stop "virtues-tenant-${SUBDOMAIN:-unknown}" 2>/dev/null || true
    systemctl stop traefik 2>/dev/null || true
}

trap 'error_exit "Script failed at line $LINENO"' ERR

log "======================================"
log "Virtues VPS Setup - Nomad + gVisor"
log "======================================"

# ============================================================================
# Environment Validation
# ============================================================================

if [ ! -f /opt/virtues/.env ]; then
    error_exit "/opt/virtues/.env not found"
fi

set -a
source /opt/virtues/.env
set +a

log "Validating environment variables..."

# Validate SUBDOMAIN
if ! [[ "${SUBDOMAIN:-}" =~ ^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$ ]]; then
    error_exit "Invalid SUBDOMAIN format: '${SUBDOMAIN:-}'"
fi

# Validate TIER
if [[ "${TIER:-}" != "free" && "${TIER:-}" != "standard" && "${TIER:-}" != "pro" ]]; then
    error_exit "Invalid TIER: '${TIER:-}'. Must be 'free', 'standard', or 'pro'."
fi

# Validate OWNER_EMAIL
if ! [[ "${OWNER_EMAIL:-}" =~ ^[^@[:space:]]+@[^@[:space:]]+\.[^@[:space:]]+$ ]]; then
    error_exit "Invalid OWNER_EMAIL format: '${OWNER_EMAIL:-}'"
fi

# Validate required secrets
required_vars=("VIRTUES_ENCRYPTION_KEY" "GHCR_REPO" "HETZNER_DNS_API_TOKEN")
for var in "${required_vars[@]}"; do
    if [ -z "${!var:-}" ]; then
        error_exit "Required environment variable $var is not set"
    fi
done

log "Subdomain: ${SUBDOMAIN}"
log "Tier: ${TIER}"
log "Owner: ${OWNER_EMAIL}"

# ============================================================================
# System Setup
# ============================================================================

log "Updating system packages..."
apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get upgrade -y -qq

log "Installing base dependencies..."
apt-get install -y -qq \
    ca-certificates \
    curl \
    gnupg \
    wget \
    unzip \
    jq \
    cifs-utils \
    sqlite3 \
    awscli \
    dnsutils \
    quota

# ============================================================================
# Install containerd (NOT Docker)
# ============================================================================

log "Installing containerd..."

install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
chmod a+r /etc/apt/keyrings/docker.gpg

echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null

apt-get update -qq
apt-get install -y -qq containerd.io

# Configure containerd
mkdir -p /etc/containerd
containerd config default > /etc/containerd/config.toml

# Enable SystemdCgroup
sed -i 's/SystemdCgroup = false/SystemdCgroup = true/' /etc/containerd/config.toml

log "containerd installed"

# ============================================================================
# Install gVisor (runsc)
# ============================================================================

log "Installing gVisor (runsc)..."

curl -fsSL https://gvisor.dev/archive.key | gpg --dearmor -o /usr/share/keyrings/gvisor-archive-keyring.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/gvisor-archive-keyring.gpg] https://storage.googleapis.com/gvisor/releases release main" | tee /etc/apt/sources.list.d/gvisor.list > /dev/null

apt-get update -qq
apt-get install -y -qq runsc

# ---------------------------------------------------------------------------
# Create tier-specific gVisor runtime shim symlinks
#
# The gVisor containerd shim resolves its config file by name:
#   containerd-shim-runsc-v1          -> /etc/containerd/runsc.toml
#   containerd-shim-runsc-standard-v1 -> /etc/containerd/runsc-standard.toml
#   containerd-shim-runsc-pro-v1      -> /etc/containerd/runsc-pro.toml
#
# This allows different overlay2 settings per tier using the same binary.
# ---------------------------------------------------------------------------

RUNSC_SHIM=$(command -v containerd-shim-runsc-v1)
ln -sf "${RUNSC_SHIM}" /usr/local/bin/containerd-shim-runsc-standard-v1
ln -sf "${RUNSC_SHIM}" /usr/local/bin/containerd-shim-runsc-pro-v1

# Register all gVisor runtimes in containerd config
cat >> /etc/containerd/config.toml << 'EOF'

# gVisor base runtime (no overlay — for stateless services like Tollbooth)
[plugins."io.containerd.grpc.v1.cri".containerd.runtimes.runsc]
  runtime_type = "io.containerd.runsc.v1"
  [plugins."io.containerd.grpc.v1.cri".containerd.runtimes.runsc.options]
    TypeUrl = "io.containerd.runsc.v1.options"
    ConfigPath = "/etc/containerd/runsc.toml"

# gVisor standard tier runtime (2GB root filesystem overlay)
[plugins."io.containerd.grpc.v1.cri".containerd.runtimes.runsc-standard]
  runtime_type = "io.containerd.runsc-standard.v1"
  [plugins."io.containerd.grpc.v1.cri".containerd.runtimes.runsc-standard.options]
    TypeUrl = "io.containerd.runsc.v1.options"
    ConfigPath = "/etc/containerd/runsc-standard.toml"

# gVisor pro tier runtime (5GB root filesystem overlay)
[plugins."io.containerd.grpc.v1.cri".containerd.runtimes.runsc-pro]
  runtime_type = "io.containerd.runsc-pro.v1"
  [plugins."io.containerd.grpc.v1.cri".containerd.runtimes.runsc-pro.options]
    TypeUrl = "io.containerd.runsc.v1.options"
    ConfigPath = "/etc/containerd/runsc-pro.toml"
EOF

# ---------------------------------------------------------------------------
# Generate gVisor runtime configurations
#
# Shared settings (platform, network, file-access, etc.) are written by a
# helper function so they stay in sync. Only overlay2 differs per tier.
# ---------------------------------------------------------------------------

generate_runsc_config() {
    local config_path="$1"
    local overlay_setting="$2"

    cat > "${config_path}" << TOML
[runsc]
  platform = "systrap"
  network = "sandbox"
  file-access = "shared"
  fsgofer-host-uds = true
  directfs = true
  overlay2 = "${overlay_setting}"
  debug = false
TOML
}

# Base runtime — no overlay (Tollbooth and other stateless services)
generate_runsc_config "/etc/containerd/runsc.toml" "none"

# Tier-specific runtimes — overlay2 enforces per-container root fs limits.
# The /data bind mount (SQLite database) is NOT governed by overlay —
# it uses ext4 project quotas set during tenant provisioning.
for tier_name in standard pro; do
    case "${tier_name}" in
        standard) OVERLAY_SIZE="2g" ;;
        pro)      OVERLAY_SIZE="5g" ;;
    esac

    mkdir -p "/var/lib/runsc/runsc-${tier_name}/overlay"
    generate_runsc_config "/etc/containerd/runsc-${tier_name}.toml" \
        "root:dir:/var/lib/runsc/runsc-${tier_name}/overlay,size=${OVERLAY_SIZE}"
done

mkdir -p /var/log/runsc

systemctl enable containerd
systemctl restart containerd

log "gVisor installed: base (no overlay), standard (2GB overlay), pro (5GB overlay)"

# ============================================================================
# Install CNI Plugins
# ============================================================================

log "Installing CNI plugins..."

CNI_VERSION="v1.4.0"
mkdir -p /opt/cni/bin
curl -fsSL "https://github.com/containernetworking/plugins/releases/download/${CNI_VERSION}/cni-plugins-linux-amd64-${CNI_VERSION}.tgz" | tar -xz -C /opt/cni/bin

# Create CNI bridge configuration
mkdir -p /etc/cni/net.d
cat > /etc/cni/net.d/10-virtues-bridge.conflist << 'EOF'
{
  "cniVersion": "1.0.0",
  "name": "virtues-bridge",
  "plugins": [
    {
      "type": "bridge",
      "bridge": "virtues0",
      "isGateway": true,
      "ipMasq": true,
      "ipam": {
        "type": "host-local",
        "ranges": [
          [{"subnet": "172.16.0.0/16"}]
        ],
        "routes": [{"dst": "0.0.0.0/0"}]
      }
    },
    {
      "type": "portmap",
      "capabilities": {"portMappings": true}
    }
  ]
}
EOF

log "CNI plugins installed"

# ============================================================================
# Install Nomad
# ============================================================================

log "Installing Nomad..."

wget -O- https://apt.releases.hashicorp.com/gpg | gpg --dearmor -o /usr/share/keyrings/hashicorp-archive-keyring.gpg
echo "deb [signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] https://apt.releases.hashicorp.com $(lsb_release -cs) main" | tee /etc/apt/sources.list.d/hashicorp.list

apt-get update -qq
apt-get install -y -qq nomad

# ============================================================================
# Install Nomad containerd Driver
# ============================================================================

log "Installing Nomad containerd driver..."

NOMAD_CONTAINERD_VERSION="0.9.4"
mkdir -p /opt/nomad/plugins
wget -q "https://github.com/Roblox/nomad-driver-containerd/releases/download/v${NOMAD_CONTAINERD_VERSION}/containerd-driver" -O /opt/nomad/plugins/containerd-driver
chmod +x /opt/nomad/plugins/containerd-driver

log "Nomad containerd driver installed"

# ============================================================================
# Configure Nomad
# ============================================================================

log "Configuring Nomad..."

mkdir -p /etc/nomad.d
mkdir -p /opt/nomad/data

# Determine node class based on tier
NODE_CLASS="${TIER}-tier"

cat > /etc/nomad.d/nomad.hcl << EOF
datacenter = "dc1"
data_dir   = "/opt/nomad/data"
bind_addr  = "0.0.0.0"

# Single server mode (bootstrap)
server {
  enabled          = true
  bootstrap_expect = 1
}

# Client configuration
client {
  enabled = true

  node_class = "${NODE_CLASS}"

  # Plugin directory
  plugin_dir = "/opt/nomad/plugins"

  # CNI configuration
  cni_path = "/opt/cni/bin"
  cni_config_dir = "/etc/cni/net.d"

  # Host volume for SQLite database only
  # Drive/Lake/Media files are stored in S3
  host_volume "tenant_data" {
    path      = "/opt/tenants/${SUBDOMAIN}/data"
    read_only = false
  }

  meta {
    tier      = "${TIER}"
    subdomain = "${SUBDOMAIN}"
  }
}

# containerd driver plugin
plugin "containerd-driver" {
  config {
    enabled           = true
    containerd_runtime = "io.containerd.runsc.v1"
    stats_interval    = "5s"
  }
}

# Telemetry
telemetry {
  collection_interval        = "10s"
  prometheus_metrics         = true
  publish_allocation_metrics = true
  publish_node_metrics       = true
}
EOF

systemctl enable nomad
systemctl start nomad

log "Nomad configured and started"

# ============================================================================
# Install Traefik
# ============================================================================

log "Installing Traefik..."

TRAEFIK_VERSION="v3.1.0"
wget -q "https://github.com/traefik/traefik/releases/download/${TRAEFIK_VERSION}/traefik_${TRAEFIK_VERSION}_linux_amd64.tar.gz" -O /tmp/traefik.tar.gz
tar -xzf /tmp/traefik.tar.gz -C /usr/local/bin traefik
chmod +x /usr/local/bin/traefik
rm /tmp/traefik.tar.gz

# Create Traefik configuration directory
mkdir -p /etc/traefik
mkdir -p /var/log/traefik

# Create Traefik static configuration
cat > /etc/traefik/traefik.yml << EOF
# Traefik Static Configuration

global:
  checkNewVersion: false
  sendAnonymousUsage: false

api:
  dashboard: false
  insecure: false

entryPoints:
  web:
    address: ":80"
    http:
      redirections:
        entryPoint:
          to: websecure
          scheme: https
  websecure:
    address: ":443"
    http:
      tls:
        certResolver: hetzner
        domains:
          - main: "virtues.com"
            sans:
              - "*.virtues.com"

providers:
  nomad:
    endpoint:
      address: "http://127.0.0.1:4646"
    exposedByDefault: false
    defaultRule: "Host(\`{{ .Name }}.virtues.com\`)"

certificatesResolvers:
  hetzner:
    acme:
      email: "${OWNER_EMAIL}"
      storage: "/etc/traefik/acme.json"
      dnsChallenge:
        provider: hetzner
        delayBeforeCheck: 0

log:
  level: WARN
  filePath: "/var/log/traefik/traefik.log"

accessLog:
  filePath: "/var/log/traefik/access.log"
  bufferingSize: 100
EOF

# Create systemd service for Traefik
cat > /etc/systemd/system/traefik.service << EOF
[Unit]
Description=Traefik Proxy
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
Environment="HETZNER_API_KEY=${HETZNER_DNS_API_TOKEN}"
ExecStart=/usr/local/bin/traefik --configFile=/etc/traefik/traefik.yml
Restart=always
RestartSec=5
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF

# Create empty acme.json with correct permissions
touch /etc/traefik/acme.json
chmod 600 /etc/traefik/acme.json

systemctl daemon-reload
systemctl enable traefik

log "Traefik installed"

# ============================================================================
# Setup Storage and Disk Quotas (ext4 project quotas)
# ============================================================================
#
# Per-tenant disk enforcement for the /data bind mount (SQLite database).
# ext4 project quotas provide kernel-enforced per-directory limits.
# The container root filesystem is separately limited by gVisor overlay2.
#
# Migration path: when 10+ tenants and monitoring/kernel-pinning in place,
# migrate to ZFS for snapshots, compression (2-3x on SQLite), and send/receive.

log "Setting up storage and disk quotas..."

# Enable ext4 project quota support on the root filesystem
ROOT_DEV=$(findmnt -n -o SOURCE /)
tune2fs -O project,quota "${ROOT_DEV}" 2>/dev/null \
    || log "WARNING: Could not enable project quota feature (may already be enabled)"

# Add prjquota to fstab mount options (survives reboot)
if ! grep -q "prjquota" /etc/fstab; then
    sed -i '/[[:space:]]\/[[:space:]]/s/\(ext4[[:space:]]\+\)\([^[:space:]]*\)/\1\2,prjquota/' /etc/fstab
    log "Added prjquota to /etc/fstab"
fi

# Remount with project quotas active
if ! findmnt -n -o OPTIONS / | grep -q "prjquota"; then
    mount -o remount,prjquota / \
        || log "WARNING: Could not remount with prjquota (may require reboot)"
fi

# Initialize and enable project quota tracking
quotacheck -Pum / 2>/dev/null || true
quotaon -P / 2>/dev/null || true

# Create tenant base directory
mkdir -p /opt/tenants

# Initialize project quota mapping files
touch /etc/projects /etc/projid

# ---------------------------------------------------------------------------
# Per-tenant provisioning script
#
# Called by:
#   1. setup.sh during initial cloud-init bootstrap (first tenant)
#   2. Atlas via SSH when onboarding additional tenants
#
# Creates the tenant data directory with ext4 project quota enforcement.
# The project_id must be unique per server — Atlas tracks this in the
# customer record (diskProjectId column). Convention: 1000 + tenantIndex.
# ---------------------------------------------------------------------------

cat > /usr/local/bin/atlas-provision-tenant.sh << 'PROVISION'
#!/bin/bash
set -euo pipefail

SUBDOMAIN="${1:?Usage: atlas-provision-tenant.sh <subdomain> <quota_gb> <project_id>}"
QUOTA_GB="${2:?Usage: atlas-provision-tenant.sh <subdomain> <quota_gb> <project_id>}"
PROJECT_ID="${3:?Usage: atlas-provision-tenant.sh <subdomain> <quota_gb> <project_id>}"

DATA_DIR="/opt/tenants/${SUBDOMAIN}/data"

# Validate inputs
if ! [[ "${SUBDOMAIN}" =~ ^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$ ]]; then
    echo "ERROR: Invalid subdomain format: ${SUBDOMAIN}" >&2
    exit 1
fi

if ! [[ "${QUOTA_GB}" =~ ^[0-9]+$ ]] || [ "${QUOTA_GB}" -eq 0 ]; then
    echo "ERROR: quota_gb must be a positive integer, got: ${QUOTA_GB}" >&2
    exit 1
fi

if ! [[ "${PROJECT_ID}" =~ ^[0-9]+$ ]]; then
    echo "ERROR: project_id must be a positive integer, got: ${PROJECT_ID}" >&2
    exit 1
fi

# Create data directory
mkdir -p "${DATA_DIR}"
chmod 700 "/opt/tenants/${SUBDOMAIN}"

# Set project ID on the directory BEFORE any files are written.
# +P makes the project ID inheritable — new files/dirs under this path
# automatically belong to this project and count against the quota.
chattr +P -p "${PROJECT_ID}" "${DATA_DIR}"

# Register project ID mapping (used by repquota for human-readable output)
grep -q "^${PROJECT_ID}:" /etc/projects 2>/dev/null \
    || echo "${PROJECT_ID}:${DATA_DIR}" >> /etc/projects
grep -q "^tenant_${SUBDOMAIN}:" /etc/projid 2>/dev/null \
    || echo "tenant_${SUBDOMAIN}:${PROJECT_ID}" >> /etc/projid

# Set hard disk quota (block limit in KB)
QUOTA_KB=$((QUOTA_GB * 1024 * 1024))
setquota -P "${PROJECT_ID}" 0 "${QUOTA_KB}" 0 0 /

echo "OK: tenant=${SUBDOMAIN} quota=${QUOTA_GB}GB project_id=${PROJECT_ID} path=${DATA_DIR}"
PROVISION
chmod +x /usr/local/bin/atlas-provision-tenant.sh

# Provision the initial tenant for this server
case "${TIER}" in
    standard) DATA_QUOTA_GB=10 ;;
    pro)      DATA_QUOTA_GB=40 ;;
    *)        DATA_QUOTA_GB=10 ;;
esac

/usr/local/bin/atlas-provision-tenant.sh "${SUBDOMAIN}" "${DATA_QUOTA_GB}" 1000

log "Storage configured: ${DATA_QUOTA_GB}GB ext4 project quota on /opt/tenants/${SUBDOMAIN}/data"

# ============================================================================
# S3 Storage Configuration
# ============================================================================
# Note: Drive, Lake, and Media files are stored in Hetzner S3-compatible storage.
# No local volume mounts needed - S3 credentials are injected via environment.
# Each tenant gets a unique prefix: users/${SUBDOMAIN}/

log "Drive storage configured for S3 (S3_PREFIX=users/${SUBDOMAIN})"

# ============================================================================
# Generate Secrets
# ============================================================================

log "Configuring secrets..."

if [ -z "${STREAM_ENCRYPTION_MASTER_KEY:-}" ]; then
    STREAM_ENCRYPTION_MASTER_KEY=$(openssl rand -hex 32)
    echo "STREAM_ENCRYPTION_MASTER_KEY=${STREAM_ENCRYPTION_MASTER_KEY}" >> /opt/virtues/.env
    log "Generated STREAM_ENCRYPTION_MASTER_KEY"
fi

if [ -z "${AUTH_SECRET:-}" ]; then
    AUTH_SECRET=$(openssl rand -base64 32)
    echo "AUTH_SECRET=${AUTH_SECRET}" >> /opt/virtues/.env
    log "Generated AUTH_SECRET"
fi

chmod 600 /opt/virtues/.env

# ============================================================================
# Create Nomad Job Specification
# ============================================================================

log "Creating Nomad job specification..."

# Determine resources based on tier
case "${TIER}" in
    free)
        MEMORY=256
        MEMORY_MAX=768
        CPU=100
        EPHEMERAL_DISK=2048
        ;;
    standard)
        MEMORY=2048
        MEMORY_MAX=2048
        CPU=1000
        EPHEMERAL_DISK=2048
        ;;
    pro)
        MEMORY=8192
        MEMORY_MAX=8192
        CPU=4000
        EPHEMERAL_DISK=5120
        ;;
esac

# Reload environment with generated secrets
set -a
source /opt/virtues/.env
set +a

cat > /opt/virtues/tenant.nomad << EOF
job "virtues-tenant-${SUBDOMAIN}" {
  datacenters = ["dc1"]
  type        = "service"

  constraint {
    attribute = "\${node.class}"
    value     = "${TIER}-tier"
  }

  group "virtues" {
    count = 1

    restart {
      attempts = 3
      interval = "5m"
      delay    = "15s"
      mode     = "fail"
    }

    network {
      port "http" {}
    }

    # Host volume for SQLite database (ext4 project quota enforced)
    volume "tenant_data" {
      type      = "host"
      source    = "tenant_data"
      read_only = false
    }

    # Ephemeral disk — sized to match gVisor overlay2 limit
    ephemeral_disk {
      size    = ${EPHEMERAL_DISK}
      migrate = false
      sticky  = false
    }

    task "core" {
      driver = "containerd-driver"

      config {
        image   = "${GHCR_REPO}/virtues-core:${TAG:-latest}"
        runtime = "io.containerd.runsc-${TIER}.v1"
      }

      # Mount volume for SQLite database
      volume_mount {
        volume      = "tenant_data"
        destination = "/data"
        read_only   = false
      }

      env {
        DATABASE_URL                  = "sqlite:/data/virtues.db"
        STATIC_DIR                    = "/app/static"
        RUST_LOG                      = "warn,virtues=info"
        RUST_ENV                      = "production"
        TIER                          = "${TIER}"
        SUBDOMAIN                     = "${SUBDOMAIN}"
        VIRTUES_ENCRYPTION_KEY        = "${VIRTUES_ENCRYPTION_KEY}"
        STREAM_ENCRYPTION_MASTER_KEY  = "${STREAM_ENCRYPTION_MASTER_KEY}"
        S3_ENDPOINT                   = "${S3_ENDPOINT:-}"
        S3_BUCKET                     = "${S3_BUCKET:-}"
        S3_ACCESS_KEY                 = "${S3_ACCESS_KEY:-}"
        S3_SECRET_KEY                 = "${S3_SECRET_KEY:-}"
        S3_PREFIX                     = "users/${SUBDOMAIN}"
        GOOGLE_CLIENT_ID              = "${GOOGLE_CLIENT_ID:-}"
        GOOGLE_CLIENT_SECRET          = "${GOOGLE_CLIENT_SECRET:-}"
        EXA_API_KEY                   = "${EXA_API_KEY:-}"
      }

      resources {
        cpu        = ${CPU}
        memory     = ${MEMORY}
        memory_max = ${MEMORY_MAX}
      }

      service {
        name = "virtues-${SUBDOMAIN}"
        port = "http"

        tags = [
          "traefik.enable=true",
          "traefik.http.routers.${SUBDOMAIN}.rule=Host(\`${SUBDOMAIN}.virtues.com\`)",
          "traefik.http.routers.${SUBDOMAIN}.entrypoints=websecure",
          "traefik.http.routers.${SUBDOMAIN}.tls.certresolver=hetzner"
        ]

        check {
          name     = "health"
          type     = "http"
          path     = "/health"
          interval = "30s"
          timeout  = "5s"
        }
      }
    }
  }
}
EOF

log "Nomad job specification created"

# ============================================================================
# Wait for Nomad and Deploy Job
# ============================================================================

log "Waiting for Nomad to be ready..."
sleep 5
TIMEOUT=60
ELAPSED=0
while [ $ELAPSED -lt $TIMEOUT ]; do
    if nomad status > /dev/null 2>&1; then
        log "Nomad is ready"
        break
    fi
    sleep 2
    ELAPSED=$((ELAPSED + 2))
done

if [ $ELAPSED -ge $TIMEOUT ]; then
    error_exit "Nomad failed to start within ${TIMEOUT}s"
fi

log "Deploying Nomad job..."
nomad job run /opt/virtues/tenant.nomad

# Wait for job to be running
log "Waiting for job to be running..."
TIMEOUT=120
ELAPSED=0
while [ $ELAPSED -lt $TIMEOUT ]; do
    STATUS=$(nomad job status "virtues-tenant-${SUBDOMAIN}" 2>/dev/null | grep -E "^Status" | awk '{print $3}' || echo "unknown")
    if [ "$STATUS" = "running" ]; then
        log "Job is running"
        break
    fi
    sleep 5
    ELAPSED=$((ELAPSED + 5))
    log "Waiting for job... status=$STATUS (${ELAPSED}s/${TIMEOUT}s)"
done

if [ $ELAPSED -ge $TIMEOUT ]; then
    log "WARNING: Job may not be fully running after ${TIMEOUT}s"
    nomad job status "virtues-tenant-${SUBDOMAIN}"
fi

# ============================================================================
# Start Traefik
# ============================================================================

log "Starting Traefik..."
systemctl start traefik

# Wait for Traefik to obtain certificate
log "Waiting for SSL certificate (this may take a minute)..."
sleep 30

# ============================================================================
# Setup Cron Jobs
# ============================================================================

log "Setting up cron jobs..."

# Daily backup
cat > /etc/cron.daily/virtues-backup << 'BACKUP'
#!/bin/bash
set -euo pipefail
cd /opt/virtues
source /opt/virtues/.env

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
BACKUP_FILE="/opt/backups/virtues-${TIMESTAMP}.db"

# SQLite backup
sqlite3 /opt/tenants/${SUBDOMAIN}/data/virtues.db ".backup ${BACKUP_FILE}"
gzip "${BACKUP_FILE}"

# Upload to S3 if configured
if [ -n "${S3_BUCKET:-}" ] && [ -n "${S3_ENDPOINT:-}" ]; then
    aws s3 cp "${BACKUP_FILE}.gz" "s3://${S3_BUCKET}/tenants/${SUBDOMAIN}/backups/" \
        --endpoint-url "${S3_ENDPOINT}" 2>/dev/null || true
fi

# Keep only 1 day locally
find /opt/backups -name "virtues-*.db.gz" -mtime +1 -delete
BACKUP
chmod +x /etc/cron.daily/virtues-backup

mkdir -p /opt/backups

# ============================================================================
# Setup Log Rotation
# ============================================================================

cat > /etc/logrotate.d/virtues << 'LOGROTATE'
/var/log/virtues-*.log /var/log/traefik/*.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    create 640 root root
}
LOGROTATE

# ============================================================================
# Final Health Check
# ============================================================================

log "Performing final health check..."
sleep 10

# Get the allocated port from Nomad
ALLOC_ID=$(nomad job status "virtues-tenant-${SUBDOMAIN}" 2>/dev/null | grep -E "^Allocations" -A 100 | grep running | awk '{print $1}' | head -1)
if [ -n "$ALLOC_ID" ]; then
    PORT=$(nomad alloc status "$ALLOC_ID" 2>/dev/null | grep -E "http.*dynamic" | awk '{print $4}' | cut -d':' -f2 | head -1)
    if [ -n "$PORT" ]; then
        if curl -sf "http://localhost:${PORT}/health" > /dev/null; then
            log "Health check passed on port ${PORT}"
        else
            log "WARNING: Health check failed on port ${PORT}"
        fi
    fi
fi

# ============================================================================
# Complete
# ============================================================================

log "======================================"
log "Virtues VPS Setup Complete!"
log "======================================"
log "Web: https://${SUBDOMAIN}.virtues.com"
log "API: https://${SUBDOMAIN}.virtues.com/api"
log "Nomad UI: http://localhost:4646 (internal only)"
log "======================================"
log "Stack: Nomad + containerd + gVisor + Traefik"
log "Tier: ${TIER}"
log "======================================"
