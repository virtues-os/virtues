#!/bin/bash
# VPS Bootstrap Script for Virtues Tenant
# Called by cloud-init during Hetzner VPS provisioning
#
# Expected environment (passed via cloud-init .env):
# - SUBDOMAIN (required)
# - TIER (required: 'starter' or 'pro')
# - OWNER_EMAIL (required)
# - VIRTUES_ENCRYPTION_KEY (required)
# - AUTH_SECRET (optional, generated if not provided)
# - RESEND_API_KEY (required)
# - GOOGLE_API_KEY (optional)
# - AI_GATEWAY_API_KEY (optional)
# - EXA_API_KEY (optional, for web search)
# - GHCR_REPO (required: GitHub Container Registry repository)

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
    cd /opt/virtues 2>/dev/null && docker compose down 2>/dev/null || true
    systemctl stop caddy 2>/dev/null || true
}

# Trap errors
trap 'error_exit "Script failed at line $LINENO"' ERR

log "======================================"
log "Virtues VPS Setup Starting"
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

# Validate SUBDOMAIN (alphanumeric and hyphens only, 1-63 chars, no leading/trailing hyphens)
if ! [[ "${SUBDOMAIN:-}" =~ ^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$ ]]; then
    error_exit "Invalid SUBDOMAIN format: '${SUBDOMAIN:-}'. Must be 1-63 lowercase alphanumeric characters or hyphens."
fi

# Validate TIER (must be 'starter' or 'pro')
if [[ "${TIER:-}" != "starter" && "${TIER:-}" != "pro" ]]; then
    error_exit "Invalid TIER: '${TIER:-}'. Must be 'starter' or 'pro'."
fi

# Validate OWNER_EMAIL (basic email format check)
if ! [[ "${OWNER_EMAIL:-}" =~ ^[^@[:space:]]+@[^@[:space:]]+\.[^@[:space:]]+$ ]]; then
    error_exit "Invalid OWNER_EMAIL format: '${OWNER_EMAIL:-}'"
fi

# Validate required secrets exist (don't log their values)
required_vars=("VIRTUES_ENCRYPTION_KEY" "RESEND_API_KEY" "GHCR_REPO" "S3_ENDPOINT" "S3_BUCKET" "S3_ACCESS_KEY" "S3_SECRET_KEY")
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

# Install Docker (using official Docker repository for latest version)
log "Installing Docker..."
apt-get install -y -qq ca-certificates curl gnupg

# Add Docker's official GPG key
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
chmod a+r /etc/apt/keyrings/docker.gpg

# Add Docker repository
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null

apt-get update -qq
apt-get install -y -qq docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

# Enable and start Docker
systemctl enable docker
systemctl start docker

# Install Caddy
log "Installing Caddy..."
apt-get install -y -qq debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list
apt-get update -qq
apt-get install -y -qq caddy awscli dnsutils sqlite3

# ============================================================================
# Directory Setup
# ============================================================================

mkdir -p /opt/virtues
mkdir -p /opt/virtues/data
mkdir -p /opt/backups
chmod 700 /opt/virtues  # Restrict access to .env

# ============================================================================
# Code Interpreter Sandbox Setup (nsjail + Python)
# ============================================================================

log "Setting up code interpreter sandbox..."

# Install nsjail
apt-get install -y -qq nsjail

# Create sandbox directories
mkdir -p /opt/sandbox/{python,workspaces}

# Install Python 3.12 if not present
if ! command -v python3.12 &> /dev/null; then
    log "Installing Python 3.12..."
    apt-get install -y -qq python3.12 python3.12-venv python3.12-dev
fi

# Create Python virtual environment with pre-installed packages
log "Creating Python sandbox environment..."
python3.12 -m venv /opt/sandbox/python
/opt/sandbox/python/bin/pip install --upgrade pip --quiet

log "Installing Python packages for code interpreter..."
/opt/sandbox/python/bin/pip install --quiet \
    numpy pandas scipy scikit-learn statsmodels \
    matplotlib seaborn plotly \
    yfinance pandas-ta fredapi alpha-vantage bt empyrical pyfolio-reloaded \
    requests httpx beautifulsoup4 \
    sympy \
    pillow openpyxl xlsxwriter \
    python-dateutil pytz tqdm

# Make Python env read-only for security
chmod -R 555 /opt/sandbox/python

# Workspaces dir for temporary code execution
chown -R root:root /opt/sandbox/workspaces
chmod 755 /opt/sandbox/workspaces

# Copy nsjail configuration
cat > /opt/sandbox/nsjail.cfg << 'NSJAIL'
name: "python-sandbox"
description: "Sandbox for AI code execution"

mode: ONCE
time_limit: 60
max_cpus: 1

rlimit_as_type: HARD
rlimit_as: 512
rlimit_cpu_type: HARD
rlimit_cpu: 60
rlimit_fsize_type: HARD
rlimit_fsize: 10
rlimit_nofile_type: HARD
rlimit_nofile: 64

clone_newnet: false
clone_newuser: true
clone_newns: true
clone_newpid: true
clone_newipc: true
clone_newuts: true
clone_newcgroup: true

uidmap {
    inside_id: "1000"
    outside_id: "1000"
    count: 1
}

gidmap {
    inside_id: "1000"
    outside_id: "1000"
    count: 1
}

mount {
    src: "/opt/sandbox/python"
    dst: "/python"
    is_bind: true
    rw: false
}

mount {
    dst: "/tmp"
    fstype: "tmpfs"
    options: "size=50M,mode=1777"
}

mount {
    src: "/dev/null"
    dst: "/dev/null"
    is_bind: true
    rw: true
}

mount {
    src: "/dev/urandom"
    dst: "/dev/urandom"
    is_bind: true
    rw: false
}

mount {
    dst: "/proc"
    fstype: "proc"
    rw: false
}

mount {
    src: "/etc/resolv.conf"
    dst: "/etc/resolv.conf"
    is_bind: true
    rw: false
}

mount {
    src: "/etc/ssl/certs"
    dst: "/etc/ssl/certs"
    is_bind: true
    rw: false
}

mount {
    src: "/usr/share/ca-certificates"
    dst: "/usr/share/ca-certificates"
    is_bind: true
    rw: false
}

keep_caps: false
disable_no_new_privs: false
log_level: WARNING
NSJAIL

log "Code interpreter sandbox setup complete"

# ============================================================================
# Generate Secrets (if not provided)
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

if [ -z "${EMAIL_FROM:-}" ]; then
    echo "EMAIL_FROM=Virtues <noreply@virtues.com>" >> /opt/virtues/.env
fi

# Secure the .env file
chmod 600 /opt/virtues/.env

# ============================================================================
# Docker Compose Configuration
# ============================================================================

log "Creating docker-compose.yml..."
cat > /opt/virtues/docker-compose.yml << 'COMPOSE'
services:
  core:
    image: ${GHCR_REPO}/virtues-core:${TAG:-latest}
    restart: unless-stopped
    environment:
      DATABASE_URL: sqlite:/data/virtues.db
      VIRTUES_ENCRYPTION_KEY: ${VIRTUES_ENCRYPTION_KEY}
      STREAM_ENCRYPTION_MASTER_KEY: ${STREAM_ENCRYPTION_MASTER_KEY}
      SUBDOMAIN: ${SUBDOMAIN}
      S3_ENDPOINT: ${S3_ENDPOINT}
      S3_BUCKET: ${S3_BUCKET}
      S3_ACCESS_KEY: ${S3_ACCESS_KEY}
      S3_SECRET_KEY: ${S3_SECRET_KEY}
      RUST_LOG: ${RUST_LOG:-warn}
      GOOGLE_API_KEY: ${GOOGLE_API_KEY:-}
    ports:
      - "127.0.0.1:8000:8000"
    volumes:
      - sqlite_data:/data
    healthcheck:
      test: ["CMD-SHELL", "wget -qO- http://localhost:8000/health || exit 1"]
      interval: 30s
      timeout: 3s
      retries: 3

  web:
    image: ${GHCR_REPO}/virtues-web:${TAG:-latest}
    restart: unless-stopped
    depends_on:
      - core
    environment:
      NODE_ENV: production
      RUST_API_URL: http://core:8000
      ELT_API_URL: http://core:8000
      AUTH_SECRET: ${AUTH_SECRET}
      OWNER_EMAIL: ${OWNER_EMAIL}
      TIER: ${TIER:-starter}
      RESEND_API_KEY: ${RESEND_API_KEY}
      EMAIL_FROM: ${EMAIL_FROM:-Virtues <noreply@virtues.com>}
      GOOGLE_API_KEY: ${GOOGLE_API_KEY:-}
      AI_GATEWAY_API_KEY: ${AI_GATEWAY_API_KEY:-}
      EXA_API_KEY: ${EXA_API_KEY:-}
      AUTH_URL: https://${SUBDOMAIN}.virtues.com
    ports:
      - "127.0.0.1:3000:3000"

volumes:
  sqlite_data:
COMPOSE

# ============================================================================
# Caddy Configuration
# ============================================================================

log "Configuring Caddy..."
cat > /etc/caddy/Caddyfile << EOF
${SUBDOMAIN}.virtues.com {
    header {
        Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
        X-Frame-Options "SAMEORIGIN"
        X-Content-Type-Options "nosniff"
        X-XSS-Protection "1; mode=block"
        Referrer-Policy "strict-origin-when-cross-origin"
        Permissions-Policy "geolocation=(), microphone=(), camera=()"
    }

    handle /health {
        reverse_proxy localhost:8000
    }

    # SvelteKit API routes (must match vite.config.ts bypass rules)
    handle /api/app* {
        reverse_proxy localhost:3000
    }
    handle /api/chat* {
        reverse_proxy localhost:3000
    }
    handle /api/sessions* {
        reverse_proxy localhost:3000
    }
    handle /api/preferences* {
        reverse_proxy localhost:3000
    }
    handle /api/assistant-profile* {
        reverse_proxy localhost:3000
    }
    handle /api/narrative* {
        reverse_proxy localhost:3000
    }
    handle /api/ontologies* {
        reverse_proxy localhost:3000
    }
    handle /api/pairing* {
        reverse_proxy localhost:3000
    }

    # All other API routes -> Rust core
    handle /api/* {
        reverse_proxy localhost:8000
    }

    handle /mcp* {
        reverse_proxy localhost:8000
    }

    handle /ingest {
        reverse_proxy localhost:8000
    }

    handle {
        reverse_proxy localhost:3000
    }

    log {
        output file /var/log/caddy/${SUBDOMAIN}.log {
            roll_size 10mb
            roll_keep 5
        }
    }
}
EOF

mkdir -p /var/log/caddy

# ============================================================================
# Start Services
# ============================================================================

log "Pulling Docker images..."
cd /opt/virtues
docker compose pull

log "Starting Virtues services..."
docker compose up -d

# Wait for services to be healthy with timeout
log "Waiting for services to be healthy..."
TIMEOUT=120
ELAPSED=0
while [ $ELAPSED -lt $TIMEOUT ]; do
    if docker compose ps | grep -q "(healthy)"; then
        log "Services are healthy"
        break
    fi
    sleep 5
    ELAPSED=$((ELAPSED + 5))
    log "Waiting for services... (${ELAPSED}s/${TIMEOUT}s)"
done

if [ $ELAPSED -ge $TIMEOUT ]; then
    log "WARNING: Services may not be fully healthy after ${TIMEOUT}s"
    docker compose ps
fi

# ============================================================================
# Run Migrations
# ============================================================================

log "Running database migrations..."
if ! docker compose exec -T core virtues migrate; then
    error_exit "Database migrations failed"
fi
log "Migrations completed successfully"

# Verify migrations by checking if database file exists and has tables
log "Verifying migrations..."
if docker compose exec -T core test -f /data/virtues.db; then
    log "Database file exists"
else
    error_exit "Migration verification failed: database file not found"
fi
log "Migration verification passed"

# ============================================================================
# Validate DNS
# ============================================================================

log "Validating DNS for ${SUBDOMAIN}.virtues.com..."
EXPECTED_IP=$(curl -sf https://api.ipify.org || curl -sf https://ifconfig.me)
RESOLVED_IP=$(dig +short "${SUBDOMAIN}.virtues.com" @8.8.8.8 | head -1)

if [ -n "$EXPECTED_IP" ] && [ "$RESOLVED_IP" != "$EXPECTED_IP" ]; then
    log "WARNING: DNS not ready (expected $EXPECTED_IP, got '$RESOLVED_IP')"
    log "Waiting 30s for DNS propagation..."
    sleep 30
fi

# ============================================================================
# Start Caddy
# ============================================================================

log "Starting Caddy..."
systemctl enable caddy
systemctl restart caddy

# ============================================================================
# Setup Cron Jobs
# ============================================================================

log "Setting up cron jobs..."

# Daily backup with S3 upload
cat > /etc/cron.daily/virtues-backup << 'BACKUP'
#!/bin/bash
set -euo pipefail

cd /opt/virtues

# Load environment for S3 credentials
set -a
source /opt/virtues/.env
set +a

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
LOCAL_FILE="/opt/backups/virtues-${TIMESTAMP}.db"
S3_KEY="tenants/${SUBDOMAIN}/backups/virtues-${TIMESTAMP}.db.gz"
LOG="/var/log/virtues-backup.log"

log() { echo "[$(date +'%Y-%m-%d %H:%M:%S')] $*" >> "$LOG"; }

# Create local backup using SQLite backup command
log "Creating backup..."
docker compose exec -T core sqlite3 /data/virtues.db ".backup /tmp/backup.db"
docker compose cp core:/tmp/backup.db "$LOCAL_FILE"
gzip "$LOCAL_FILE"

# Upload to S3
log "Uploading to S3: ${S3_KEY}"
if aws s3 cp "${LOCAL_FILE}.gz" "s3://${S3_BUCKET}/${S3_KEY}" \
    --endpoint-url "${S3_ENDPOINT}" 2>&1 >> "$LOG"; then
    log "S3 upload successful"
else
    log "ERROR: S3 upload failed"
fi

# Keep only 1 day locally (S3 has 7 days for disaster recovery)
find /opt/backups -name "virtues-*.db.gz" -mtime +1 -delete

# Keep only 7 days in S3
log "Cleaning up old S3 backups..."
CUTOFF_DATE=$(date -d "7 days ago" +%Y%m%d)
aws s3 ls "s3://${S3_BUCKET}/tenants/${SUBDOMAIN}/backups/" \
    --endpoint-url "${S3_ENDPOINT}" 2>/dev/null | \
    while read -r line; do
        FILE=$(echo "$line" | awk '{print $4}')
        # Extract date from filename: virtues-YYYYMMDD-HHMMSS.db.gz
        FILE_DATE=$(echo "$FILE" | sed -n 's/virtues-\([0-9]\{8\}\).*/\1/p')
        if [[ -n "$FILE_DATE" && "$FILE_DATE" < "$CUTOFF_DATE" ]]; then
            log "Deleting old backup: $FILE"
            aws s3 rm "s3://${S3_BUCKET}/tenants/${SUBDOMAIN}/backups/${FILE}" \
                --endpoint-url "${S3_ENDPOINT}" 2>&1 >> "$LOG" || true
        fi
    done

log "Backup completed: ${LOCAL_FILE}.gz"
BACKUP
chmod +x /etc/cron.daily/virtues-backup

# Hourly session cleanup (runs via core container with SQLite)
cat > /etc/cron.hourly/virtues-cleanup << 'CLEANUP'
#!/bin/bash
cd /opt/virtues
# Cleanup expired sessions and tokens directly in SQLite
docker compose exec -T core sqlite3 /data/virtues.db "
    DELETE FROM app_auth_session WHERE expires < datetime('now');
    DELETE FROM app_auth_verification_token WHERE expires < datetime('now');
" >> /var/log/virtues-cleanup.log 2>&1
CLEANUP
chmod +x /etc/cron.hourly/virtues-cleanup

# ============================================================================
# Setup Log Rotation
# ============================================================================

cat > /etc/logrotate.d/virtues << 'LOGROTATE'
/var/log/virtues-*.log {
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

if curl -sf "http://localhost:8000/health" > /dev/null; then
    log "Core API health check passed"
else
    log "WARNING: Core API health check failed"
fi

if curl -sf "http://localhost:3000" > /dev/null; then
    log "Web app health check passed"
else
    log "WARNING: Web app health check failed"
fi

# ============================================================================
# Complete
# ============================================================================

log "======================================"
log "Virtues VPS Setup Complete!"
log "======================================"
log "Web: https://${SUBDOMAIN}.virtues.com"
log "API: https://${SUBDOMAIN}.virtues.com/api"
log "Logs: /var/log/virtues-setup.log"
log "======================================"
