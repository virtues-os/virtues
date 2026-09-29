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
# DEPLOY_HOST is an SSH alias for the server (default: virtues-cloud). The ref
# must be pushed: the server fetches it from GitHub.
set -euo pipefail

SVC="${1:-}"
REF="${2:-}"
HOST="${DEPLOY_HOST:-virtues-cloud}"

case "$SVC" in
  virtues-api)   PORT=9002; SMOKE=9003; ENVF=/etc/virtues/api.env;   PORTVAR=VIRTUES_API_PORT;   MOUNTS="-v /srv/maps:/srv/maps:ro" ;;
  virtues-atlas) PORT=9100; SMOKE=9103; ENVF=/etc/virtues/atlas.env; PORTVAR=VIRTUES_ATLAS_PORT; MOUNTS="" ;;
  *) echo "usage: $0 virtues-api|virtues-atlas <ref>|--rollback" >&2; exit 2 ;;
esac
[ -n "$REF" ] || { echo "deploy: name the commit to deploy (a SHA, tag or branch that is pushed)" >&2; exit 2; }

RUN="--restart unless-stopped --network host --env-file $ENVF $MOUNTS"

if [ "$REF" = "--rollback" ]; then
  echo "∴ rolling $SVC back to $SVC:previous on $HOST"
  ssh "$HOST" "set -e; sudo docker image inspect $SVC:previous >/dev/null
    sudo docker rm -f $SVC >/dev/null; sudo docker run -d --name $SVC $RUN $SVC:previous >/dev/null
    for i in \$(seq 1 60); do curl -sf localhost:$PORT/health >/dev/null && break; sleep 0.5; done
    curl -sf localhost:$PORT/health >/dev/null && echo '✓ $SVC is back on :previous'"
  exit 0
fi

git fetch -q origin
SHA=$(git rev-parse --verify "$REF^{commit}")
[ -n "$(git branch -r --contains "$SHA" 2>/dev/null)" ] \
  || { echo "deploy: $SHA is not on any pushed branch; push it first (the server builds from GitHub)" >&2; exit 1; }
TAG="${SHA:0:12}"
echo "∴ deploying $SVC at $TAG ($(git log -1 --format=%s "$SHA")) to $HOST"

ssh "$HOST" bash -s <<EOF
set -euo pipefail
[ -d ~/deploy-src/.git ] || git clone -q https://github.com/virtues-os/virtues.git ~/deploy-src
cd ~/deploy-src && git fetch -q origin && git -c advice.detachedHead=false checkout -q --force $SHA

echo "· building $SVC:$TAG (a release Rust build, several minutes)"
sudo docker build -q -f services/$SVC/Dockerfile -t $SVC:$TAG . >/dev/null

echo "· smoke test on :$SMOKE"
sudo docker rm -f $SVC-smoke >/dev/null 2>&1 || true
sudo docker run -d --name $SVC-smoke --network host --env-file $ENVF -e $PORTVAR=$SMOKE $MOUNTS $SVC:$TAG >/dev/null
ok=0; for i in \$(seq 1 60); do curl -sf localhost:$SMOKE/health >/dev/null && { ok=1; break; }; sleep 1; done
if [ \$ok != 1 ]; then
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
ok=0; for i in \$(seq 1 60); do curl -sf localhost:$PORT/health >/dev/null && { ok=1; break; }; sleep 0.5; done
if [ \$ok != 1 ]; then
  echo "✖ $SVC:$TAG did not come up; rolling back to \$prev" >&2
  sudo docker rm -f $SVC >/dev/null; sudo docker run -d --name $SVC $RUN $SVC:previous >/dev/null; exit 1
fi
echo "✓ $SVC is live at $TAG (previous image kept as $SVC:previous)"
EOF
