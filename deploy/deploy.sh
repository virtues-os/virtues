#!/usr/bin/env bash
# deploy.sh - Deploy new image versions to all Nomad clusters
#
# Called by GitHub Actions after image build, or manually:
#   IMAGE_TAG=abc123 bash deploy/deploy.sh
#   IMAGE_TAG=abc123 bash deploy/deploy.sh --dry-run
#
# Requires:
#   - SSH key at ~/.ssh/deploy_key (CI sets this up)
#   - deploy/clusters.json with cluster definitions
#   - IMAGE_TAG env var (git SHA)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLUSTERS_FILE="$SCRIPT_DIR/clusters.json"
DRY_RUN=false

if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN=true
  echo "[dry-run] No changes will be made"
fi

if [[ -z "${IMAGE_TAG:-}" ]]; then
  echo "ERROR: IMAGE_TAG environment variable is required"
  exit 1
fi

# Validate IMAGE_TAG format (git SHA or "latest")
if [[ ! "$IMAGE_TAG" =~ ^[a-f0-9]{7,40}$ && "$IMAGE_TAG" != "latest" ]]; then
  echo "ERROR: IMAGE_TAG must be a git SHA or 'latest', got: $IMAGE_TAG"
  exit 1
fi

if [[ ! -f "$CLUSTERS_FILE" ]]; then
  echo "ERROR: $CLUSTERS_FILE not found"
  exit 1
fi

echo "=== Deploying image tag: $IMAGE_TAG ==="
echo ""

# Read clusters from JSON
CLUSTER_COUNT=$(jq '.clusters | length' "$CLUSTERS_FILE")

if [[ "$CLUSTER_COUNT" -eq 0 ]]; then
  echo "WARNING: No clusters configured in $CLUSTERS_FILE"
  exit 0
fi

FAILED=0

for i in $(seq 0 $((CLUSTER_COUNT - 1))); do
  NAME=$(jq -r ".clusters[$i].name" "$CLUSTERS_FILE")
  HOST=$(jq -r ".clusters[$i].host" "$CLUSTERS_FILE")
  USER=$(jq -r ".clusters[$i].user" "$CLUSTERS_FILE")

  # Validate HOST: must be an IP address or hostname (no shell metacharacters)
  if [[ ! "$HOST" =~ ^[a-zA-Z0-9._-]+$ ]]; then
    echo "ERROR: Invalid host value in clusters.json: $HOST"
    exit 1
  fi

  # Validate USER: must be alphanumeric with hyphens/underscores
  if [[ ! "$USER" =~ ^[a-zA-Z0-9_-]+$ ]]; then
    echo "ERROR: Invalid user value in clusters.json: $USER"
    exit 1
  fi

  echo "--- Deploying to: $NAME ($HOST) ---"

  if [[ "$HOST" == "YOUR_SERVER_IP" ]]; then
    echo "  SKIPPED: Placeholder IP, configure deploy/clusters.json"
    continue
  fi

  SSH_OPTS=(-i ~/.ssh/deploy_key -o StrictHostKeyChecking=accept-new -o ConnectTimeout=10)

  if $DRY_RUN; then
    echo "  [dry-run] Would SSH to ${USER}@${HOST} and run rollout.sh $IMAGE_TAG"
    continue
  fi

  # Ensure deploy directory exists on remote
  ssh "${SSH_OPTS[@]}" "${USER}@${HOST}" "mkdir -p /tmp/virtues-deploy"

  # Copy rollout script and Nomad job files to server
  scp "${SSH_OPTS[@]}" \
    "$SCRIPT_DIR/rollout.sh" \
    "$SCRIPT_DIR/nomad/tollbooth.nomad" \
    "$SCRIPT_DIR/nomad/tenant.nomad.hcl" \
    "${USER}@${HOST}:/tmp/virtues-deploy/"

  # Run rollout
  if ssh "${SSH_OPTS[@]}" "${USER}@${HOST}" "bash /tmp/virtues-deploy/rollout.sh '$IMAGE_TAG'"; then
    echo "  SUCCESS: $NAME"
  else
    echo "  FAILED: $NAME"
    FAILED=$((FAILED + 1))
  fi

  echo ""
done

if [[ $FAILED -gt 0 ]]; then
  echo "=== DEPLOY COMPLETED WITH $FAILED FAILURE(S) ==="
  exit 1
fi

echo "=== DEPLOY COMPLETE ==="
