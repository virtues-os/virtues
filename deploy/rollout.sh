#!/usr/bin/env bash
# rollout.sh - Update all Nomad jobs on this server to a new image tag
#
# Runs on each Nomad server (copied there by deploy.sh).
# Usage: bash rollout.sh <image-tag>
#
# What it does:
#   1. Updates the tollbooth system job
#   2. Discovers all running virtues-tenant-* jobs
#   3. Updates each tenant job with the new image tag
#   4. Nomad's rolling update stanza handles per-tenant rollout

set -euo pipefail

TAG="${1:?Usage: rollout.sh <image-tag>}"
DEPLOY_DIR="/tmp/virtues-deploy"
GHCR_REPO="ghcr.io/virtues-os"

# Validate tag format (git SHA or "latest")
if [[ ! "$TAG" =~ ^[a-f0-9]{7,40}$ && "$TAG" != "latest" ]]; then
  echo "[rollout] ERROR: Invalid tag format: $TAG"
  exit 1
fi

echo "[rollout] Starting rollout to tag: $TAG"
echo "[rollout] Nomad server: $(nomad version 2>/dev/null || echo 'not found')"

# --------------------------------------------------------------------------
# 1. Update Tollbooth (system job - one per host)
# --------------------------------------------------------------------------
echo ""
echo "[tollbooth] Updating tollbooth system job..."

if OUTPUT=$(nomad job run \
  -var="tag=$TAG" \
  -var="ghcr_repo=$GHCR_REPO" \
  "$DEPLOY_DIR/tollbooth.nomad" 2>&1); then
  echo "[tollbooth] OK"
  echo "$OUTPUT"
else
  echo "[tollbooth] WARNING: Failed to update tollbooth (continuing with tenants)"
  echo "$OUTPUT"
fi

# --------------------------------------------------------------------------
# 2. Discover and update all tenant jobs
# --------------------------------------------------------------------------
echo ""
echo "[tenants] Discovering tenant jobs..."

# List all running jobs that match the tenant naming pattern
TENANT_JOBS=$(nomad job status 2>/dev/null | \
  awk '/virtues-tenant-/ {print $1}' || echo "")

if [[ -z "$TENANT_JOBS" ]]; then
  echo "[tenants] No tenant jobs found. Nothing to update."
  echo "[rollout] Done."
  exit 0
fi

TOTAL=$(echo "$TENANT_JOBS" | wc -l | tr -d ' ')
echo "[tenants] Found $TOTAL tenant job(s)"

SUCCESS=0
FAILED=0

NEW_IMAGE="${GHCR_REPO}/virtues-core:${TAG}"

for JOB_ID in $TENANT_JOBS; do
  SUBDOMAIN="${JOB_ID#virtues-tenant-}"
  echo "  [$SUBDOMAIN] -> $NEW_IMAGE"

  # Fetch running job spec, swap only the image, resubmit via Nomad API.
  # This preserves all existing env vars, volumes, and config.
  UPDATED=$(nomad job inspect "$JOB_ID" 2>/dev/null | \
    jq --arg img "$NEW_IMAGE" '{Job: (.Job | .TaskGroups[0].Tasks[0].Config.image = $img)}')

  if [[ -z "$UPDATED" || "$UPDATED" == "null" ]]; then
    echo "  [$SUBDOMAIN] FAILED (could not inspect job)"
    FAILED=$((FAILED + 1))
    continue
  fi

  if OUTPUT=$(curl -sf -X POST "http://127.0.0.1:4646/v1/jobs" \
    -H "Content-Type: application/json" \
    -d "$UPDATED" 2>&1); then
    SUCCESS=$((SUCCESS + 1))
    echo "  [$SUBDOMAIN] OK"
  else
    echo "  [$SUBDOMAIN] FAILED"
    echo "  $OUTPUT"
    FAILED=$((FAILED + 1))
  fi
done

echo ""
echo "[rollout] Complete: $SUCCESS succeeded, $FAILED failed (of $TOTAL)"

if [[ $FAILED -gt 0 ]]; then
  exit 1
fi
