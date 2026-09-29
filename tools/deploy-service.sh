#!/usr/bin/env bash
# Deploy a Virtues-operated cloud service (atlas or virtues-api) to its server.
#
#   tools/deploy-service.sh virtues-api <ref>
#   tools/deploy-service.sh virtues-atlas <ref>
#   tools/deploy-service.sh virtues-api --rollback
#
# The image is built ON the server from one commit of the public repo, so what
# runs is exactly that commit and nothing from anyone's working tree. It is
# smoke-tested on a spare port against the live config, then swapped in (about
# a second of gap). The image it replaces is kept as <service>:previous, and a
# new container that does not come up healthy is rolled back automatically.
#
# The smoke test runs against the LIVE database, and both services migrate on
# startup: a failed smoke test leaves the running service untouched, but any
# migration the new image applied has already applied. Migrations are
# append-only, so the old image keeps working on the newer schema.
#
# One deploy per service at a time: a second one waits for nothing, it refuses.
#
# DEPLOY_HOST is an SSH alias for the server (default: virtues-cloud). The ref
# must be pushed: the server fetches it from GitHub. A branch name resolves to
# origin/<branch>, never the local branch, which may be ahead or behind.
set -euo pipefail

SVC="${1:-}"
REF="${2:-}"
HOST="${DEPLOY_HOST:-virtues-cloud}"

case "$SVC" in
  virtues-api)   PORT=9002; SMOKE=9003; READY=/ready;  ENVF=/etc/virtues/api.env;   PORTVAR=VIRTUES_API_PORT;   MOUNTS="-v /srv/maps:/srv/maps:ro" ;;
  virtues-atlas) PORT=9100; SMOKE=9103; READY=/health; ENVF=/etc/virtues/atlas.env; PORTVAR=VIRTUES_ATLAS_PORT; MOUNTS="" ;;
  *) echo "usage: $0 virtues-api|virtues-atlas <ref>|--rollback" >&2; exit 2 ;;
esac
[ -n "$REF" ] || { echo "deploy: name the commit to deploy (a SHA, tag or branch that is pushed)" >&2; exit 2; }

RUN="--restart unless-stopped --network host --env-file $ENVF $MOUNTS"
# Shared by both paths: take the per-service lock, and wait for a port to answer.
PRELUDE="exec 9>/tmp/deploy-$SVC.lock
flock -n 9 || { echo '✖ another deploy of $SVC is running on this server' >&2; exit 1; }
up() { for i in \$(seq 1 60); do curl -sf localhost:\$1$READY >/dev/null && return 0; sleep 1; done; return 1; }"

if [ "$REF" = "--rollback" ]; then
  echo "∴ rolling $SVC back to $SVC:previous on $HOST"
  ssh "$HOST" bash -s <<EOF
set -euo pipefail
$PRELUDE
sudo docker image inspect $SVC:previous >/dev/null 2>&1 \
  || { echo "✖ there is no $SVC:previous on this server; nothing to roll back to" >&2; exit 1; }
sudo docker rm -f $SVC >/dev/null 2>&1 || true
sudo docker run -d --name $SVC $RUN $SVC:previous >/dev/null
up $PORT || { echo "✖ $SVC:previous did not come up either; $SVC is DOWN" >&2; sudo docker logs --tail 30 $SVC >&2 || true; exit 1; }
echo "✓ $SVC is back on :previous"
EOF
  exit 0
fi

git fetch -q origin
SHA=$(git rev-parse --verify -q "origin/$REF^{commit}" || git rev-parse --verify "$REF^{commit}")
[ -n "$(git branch -r --contains "$SHA" 2>/dev/null)" ] \
  || { echo "deploy: $SHA is not on any pushed branch; push it first (the server builds from GitHub)" >&2; exit 1; }
TAG="${SHA:0:12}"
echo "∴ deploying $SVC at $TAG ($(git log -1 --format=%s "$SHA")) to $HOST"

ssh "$HOST" bash -s <<EOF
set -euo pipefail
$PRELUDE
[ -d ~/deploy-src/.git ] || git clone -q https://github.com/virtues-os/virtues.git ~/deploy-src
cd ~/deploy-src && git fetch -q origin && git -c advice.detachedHead=false checkout -q --force $SHA

echo "· building $SVC:$TAG (a release Rust build, several minutes)"
sudo docker build -q -f services/$SVC/Dockerfile -t $SVC:$TAG . >/dev/null

echo "· smoke test on :$SMOKE (migrates the live database)"
sudo docker rm -f $SVC-smoke >/dev/null 2>&1 || true
if ss -Hltn "sport = :$SMOKE" | grep -q .; then
  echo "✖ something already listens on :$SMOKE; the smoke test needs it free" >&2; exit 1
fi
sudo docker run -d --name $SVC-smoke --network host --env-file $ENVF -e $PORTVAR=$SMOKE $MOUNTS $SVC:$TAG >/dev/null
if ! up $SMOKE; then
  echo "✖ $SVC:$TAG failed its smoke test; the running service is untouched" >&2
  sudo docker logs --tail 30 $SVC-smoke >&2 || true
  sudo docker rm -f $SVC-smoke >/dev/null; exit 1
fi
sudo docker rm -f $SVC-smoke >/dev/null

echo "· swapping in"
prev=\$(sudo docker inspect --format '{{.Config.Image}}' $SVC 2>/dev/null || true)
[ -n "\$prev" ] && [ "\$prev" != "$SVC:$TAG" ] && sudo docker tag "\$prev" $SVC:previous
sudo docker rm -f $SVC >/dev/null 2>&1 || true
sudo docker run -d --name $SVC $RUN $SVC:$TAG >/dev/null
if ! up $PORT; then
  echo "✖ $SVC:$TAG did not come up; rolling back to $SVC:previous" >&2
  sudo docker logs --tail 30 $SVC >&2 || true
  sudo docker rm -f $SVC >/dev/null
  sudo docker image inspect $SVC:previous >/dev/null 2>&1 \
    || { echo "✖ there is no $SVC:previous to roll back to; $SVC is DOWN" >&2; exit 1; }
  sudo docker run -d --name $SVC $RUN $SVC:previous >/dev/null
  up $PORT && echo "· $SVC is back on :previous" >&2 \
    || echo "✖ $SVC:previous did not come up either; $SVC is DOWN" >&2
  exit 1
fi
echo "✓ $SVC is live at $TAG (previous image kept as $SVC:previous)"
EOF
