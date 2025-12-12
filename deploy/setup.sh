#!/bin/bash
# VPS Bootstrap Script for Virtues Tenant
# Called by cloud-init during Hetzner VPS provisioning
#
# Expected environment (passed via cloud-init .env):
# - SUBDOMAIN (required)
# - TIER (required: 'starter' or 'pro')
# - OWNER_EMAIL (required)
# - DB_PASSWORD (required)
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
required_vars=("DB_PASSWORD" "VIRTUES_ENCRYPTION_KEY" "RESEND_API_KEY" "GHCR_REPO" "S3_ENDPOINT" "S3_BUCKET" "S3_ACCESS_KEY" "S3_SECRET_KEY")
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
apt-get install -y -qq caddy awscli

# ============================================================================
# Directory Setup
# ============================================================================

mkdir -p /opt/virtues
mkdir -p /opt/backups
chmod 700 /opt/virtues  # Restrict access to .env

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
  postgres:
    image: ghcr.io/virtues-os/virtues-postgres:latest
    restart: unless-stopped
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: ${DB_PASSWORD}
      POSTGRES_DB: virtues
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 10s
      timeout: 5s
      retries: 5

  core:
    image: ${GHCR_REPO}/virtues-core:${TAG:-latest}
    restart: unless-stopped
    depends_on:
      postgres:
        condition: service_healthy
    environment:
      DATABASE_URL: postgresql://postgres:${DB_PASSWORD}@postgres:5432/virtues
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
      DATABASE_URL: postgresql://postgres:${DB_PASSWORD}@postgres:5432/virtues
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
  postgres_data:
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

# Verify migrations
log "Verifying migrations..."
TABLES=$(docker compose exec -T postgres psql -U postgres -d virtues -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'app'")
if [ "$TABLES" -lt 5 ]; then
    error_exit "Migration verification failed: expected at least 5 tables in app schema, found $TABLES"
fi
log "Migration verification passed ($TABLES tables in app schema)"

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
LOCAL_FILE="/opt/backups/virtues-${TIMESTAMP}.sql.gz"
S3_KEY="tenants/${SUBDOMAIN}/backups/virtues-${TIMESTAMP}.sql.gz"
LOG="/var/log/virtues-backup.log"

log() { echo "[$(date +'%Y-%m-%d %H:%M:%S')] $*" >> "$LOG"; }

# Create local backup
log "Creating backup..."
docker compose exec -T postgres pg_dump -U postgres virtues | gzip > "$LOCAL_FILE"

# Upload to S3
log "Uploading to S3: ${S3_KEY}"
if aws s3 cp "$LOCAL_FILE" "s3://${S3_BUCKET}/${S3_KEY}" \
    --endpoint-url "${S3_ENDPOINT}" 2>&1 >> "$LOG"; then
    log "S3 upload successful"
else
    log "ERROR: S3 upload failed"
fi

# Keep only 1 day locally (S3 has 7 days for disaster recovery)
find /opt/backups -name "virtues-*.sql.gz" -mtime +1 -delete

# Keep only 7 days in S3
log "Cleaning up old S3 backups..."
CUTOFF_DATE=$(date -d "7 days ago" +%Y%m%d)
aws s3 ls "s3://${S3_BUCKET}/tenants/${SUBDOMAIN}/backups/" \
    --endpoint-url "${S3_ENDPOINT}" 2>/dev/null | \
    while read -r line; do
        FILE=$(echo "$line" | awk '{print $4}')
        # Extract date from filename: virtues-YYYYMMDD-HHMMSS.sql.gz
        FILE_DATE=$(echo "$FILE" | sed -n 's/virtues-\([0-9]\{8\}\).*/\1/p')
        if [[ -n "$FILE_DATE" && "$FILE_DATE" < "$CUTOFF_DATE" ]]; then
            log "Deleting old backup: $FILE"
            aws s3 rm "s3://${S3_BUCKET}/tenants/${SUBDOMAIN}/backups/${FILE}" \
                --endpoint-url "${S3_ENDPOINT}" 2>&1 >> "$LOG" || true
        fi
    done

log "Backup completed: $LOCAL_FILE"
BACKUP
chmod +x /etc/cron.daily/virtues-backup

# Hourly session cleanup (runs via web container)
cat > /etc/cron.hourly/virtues-cleanup << 'CLEANUP'
#!/bin/bash
cd /opt/virtues
# Cleanup expired sessions and tokens directly in postgres
docker compose exec -T postgres psql -U postgres -d virtues -c "
    DELETE FROM app.auth_session WHERE expires < NOW();
    DELETE FROM app.auth_verification_token WHERE expires < NOW();
" >> /var/log/virtues-cleanup.log 2>&1
CLEANUP
chmod +x /etc/cron.hourly/virtues-cleanup

# ============================================================================
# Setup Automatic Updates (systemd timer)
# ============================================================================

log "Setting up automatic update checker..."

# Create the update check script
cat > /opt/virtues/update-check.sh << 'UPDATECHECK'
#!/bin/bash
set -euo pipefail
LOG="/var/log/virtues-update.log"

log() { echo "[$(date +'%Y-%m-%d %H:%M:%S')] $*" >> "$LOG"; }

cd /opt/virtues

# Load environment
set -a
source /opt/virtues/.env
set +a

log "Checking for updates..."

# Pull images quietly first to check if there are updates
OLD_CORE=$(docker compose images core --quiet 2>/dev/null || echo "")
OLD_WEB=$(docker compose images web --quiet 2>/dev/null || echo "")

docker compose pull --quiet 2>/dev/null || true

NEW_CORE=$(docker compose images core --quiet 2>/dev/null || echo "")
NEW_WEB=$(docker compose images web --quiet 2>/dev/null || echo "")

# Check if any images changed
if [ "$OLD_CORE" != "$NEW_CORE" ] || [ "$OLD_WEB" != "$NEW_WEB" ]; then
    log "New images found, updating..."

    # Stop services gracefully
    docker compose stop web core

    # Start with new images
    docker compose up -d

    # Wait for core to be healthy
    sleep 15

    # Run migrations
    if docker compose exec -T core virtues migrate 2>&1 >> "$LOG"; then
        log "Update completed successfully"
    else
        log "ERROR: Migration failed, but services are running"
    fi

    # Cleanup old images
    docker image prune -f >> "$LOG" 2>&1 || true
else
    log "No updates available"
fi
UPDATECHECK
chmod +x /opt/virtues/update-check.sh

# Create systemd service for updates
cat > /etc/systemd/system/virtues-update.service << 'UPDATESERVICE'
[Unit]
Description=Virtues Update Check
After=docker.service
Requires=docker.service

[Service]
Type=oneshot
ExecStart=/opt/virtues/update-check.sh
WorkingDirectory=/opt/virtues
StandardOutput=append:/var/log/virtues-update.log
StandardError=append:/var/log/virtues-update.log
UPDATESERVICE

# Create systemd timer (default 8:00 UTC = 3 AM Central)
# The hour is configurable via user settings and updated by the web app
cat > /etc/systemd/system/virtues-update.timer << 'UPDATETIMER'
[Unit]
Description=Daily Virtues Update Check

[Timer]
OnCalendar=*-*-* 08:00:00
Persistent=true
RandomizedDelaySec=300

[Install]
WantedBy=timers.target
UPDATETIMER

# Enable and start the timer
systemctl daemon-reload
systemctl enable virtues-update.timer
systemctl start virtues-update.timer

log "Automatic updates configured (daily at 08:00 UTC)"

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
