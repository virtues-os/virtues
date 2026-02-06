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
    dnsutils

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

# Add runsc runtime to containerd config
cat >> /etc/containerd/config.toml << 'EOF'

# gVisor runtime configuration
[plugins."io.containerd.grpc.v1.cri".containerd.runtimes.runsc]
  runtime_type = "io.containerd.runsc.v1"
  [plugins."io.containerd.grpc.v1.cri".containerd.runtimes.runsc.options]
    TypeUrl = "io.containerd.runsc.v1.options"
    ConfigPath = "/etc/containerd/runsc.toml"
EOF

# Create runsc configuration
cat > /etc/containerd/runsc.toml << 'EOF'
[runsc]
  # Platform: systrap is most compatible, kvm is faster if available
  platform = "systrap"

  # Network: use sandbox mode (network namespace from containerd/CNI)
  network = "sandbox"

  # File access: shared required for SQLite WAL mode
  file-access = "shared"
  fsgofer-host-uds = true

  # Performance optimizations
  directfs = true

  # Disable overlay for direct bind mount access
  overlay2 = "none"

  # Logging (disable in production for performance)
  debug = false
EOF

mkdir -p /var/log/runsc

systemctl enable containerd
systemctl restart containerd

log "gVisor (runsc) installed and configured"

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
# Setup ZFS (if available) or fallback to standard directories
# ============================================================================

log "Setting up storage..."

# Create tenant data directory
mkdir -p /opt/tenants/${SUBDOMAIN}/data
chmod 700 /opt/tenants/${SUBDOMAIN}

# Check if ZFS is available
if command -v zfs &> /dev/null && zpool list &> /dev/null 2>&1; then
    log "ZFS detected, creating dataset..."

    # Determine quota based on tier
    case "${TIER}" in
        free)     QUOTA="1G" ;;
        standard) QUOTA="20G" ;;
        pro)      QUOTA="100G" ;;
    esac

    # Create ZFS dataset if tank pool exists
    if zpool list tank &> /dev/null 2>&1; then
        zfs create -o quota=${QUOTA} -o mountpoint=/opt/tenants/${SUBDOMAIN}/data tank/tenants/${SUBDOMAIN} 2>/dev/null || true
        log "ZFS dataset created with ${QUOTA} quota"
    fi
else
    log "ZFS not available, using standard directories"
fi

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
        ;;
    standard)
        MEMORY=2048
        MEMORY_MAX=2048
        CPU=1000
        ;;
    pro)
        MEMORY=8192
        MEMORY_MAX=8192
        CPU=4000
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
      mode = "bridge"

      port "http" {
        to = 8000
      }
    }

    # Host volume for SQLite database only
    volume "tenant_data" {
      type      = "host"
      source    = "tenant_data"
      read_only = false
    }

    task "core" {
      driver = "containerd-driver"

      config {
        image   = "${GHCR_REPO}/virtues-core:${TAG:-latest}"
        runtime = "io.containerd.runsc.v1"
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
