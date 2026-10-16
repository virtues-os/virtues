#!/bin/sh
# Install (or reinstall) the demo life on a demo box. Run as root on the box:
#
#   sudo sh deploy/demo/install.sh virtues-core/seeds/demo/life.sql.gz
#
# DESTRUCTIVE: it empties the box's life (every data_* and wiki_* row, chats,
# pages, projects) before loading the demo one. It refuses unless the box is
# marked as a demo box with VIRTUES_DEMO_BOX=1 in /var/lib/virtues/virtues.env,
# so it can never run against someone's real record by a slip of the host name.
#
# Keeps: auth, paired devices (a reviewer's pairing survives), settings, the
# box's system applets and its real weather.
set -eu

LIFE=${1:?usage: install.sh <life.sql.gz>}
HERE=$(cd "$(dirname "$0")" && pwd)
ENVF=/var/lib/virtues/virtues.env
SHARE=/usr/local/share/virtues/demo
PSQL="sudo -u postgres psql -X -q -v ON_ERROR_STOP=1 -d virtues"

grep -qx 'VIRTUES_DEMO_BOX=1' "$ENVF" 2>/dev/null || {
  echo "refusing: $ENVF does not mark this as a demo box (VIRTUES_DEMO_BOX=1)" >&2; exit 1; }
[ -r "$LIFE" ] || { echo "cannot read $LIFE" >&2; exit 1; }

echo "→ backup"
install -d -o postgres /var/backups/virtues
sudo -u postgres pg_dump -Fc -d virtues -f "/var/backups/virtues/pre-demo-install-$(date -u +%Y%m%d%H%M).dump"

echo "→ stop the demo timers"
for u in virtues-demo-place virtues-demo-reset virtues-demo3y-reanchor virtues-demo3y-reset; do
  systemctl disable --now "$u.timer" 2>/dev/null || true
done

echo "→ clear, load, place (one transaction)"
{ cat "$HERE/clear.sql"; gunzip -c "$LIFE"; cat "$HERE/place.sql"; cat "$HERE/reset.sql"; } \
  | $PSQL -1 -f - >/dev/null

echo "→ the box's own day writer stays off: the life is already written"
$PSQL -c "UPDATE app_applets SET enabled = false WHERE id = 'applet_day_summary_eod'"

echo "→ units"
install -d "$SHARE"
install -m 644 "$HERE/place.sql" "$HERE/reset.sql" "$SHARE/"
install -m 644 "$HERE"/units/virtues-demo-*.service "$HERE"/units/virtues-demo-*.timer /etc/systemd/system/
install -d /etc/virtues/health.d
install -m 644 "$HERE/health.sh" /etc/virtues/health.d/demo.sh
# Retired schemes: the base seed's re-anchor and demo3y's own timers. Moved
# aside (mask refuses a unit file that exists in /etc) and masked, so an
# upgrade or a stray `enable` can never start a second placement.
for u in virtues-demo-reanchor virtues-demo3y-reanchor virtues-demo3y-reset; do
  for k in service timer; do
    [ -f "/etc/systemd/system/$u.$k" ] && mv "/etc/systemd/system/$u.$k" "/usr/local/share/virtues/retired-$u.$k" || true
  done
  rm -rf "/etc/systemd/system/$u.service.d"
done
systemctl daemon-reload
for u in virtues-demo-reanchor virtues-demo3y-reanchor virtues-demo3y-reset; do
  systemctl mask "$u.timer" "$u.service" >/dev/null 2>&1 || true
done
systemctl enable --now virtues-demo-place.timer virtues-demo-reset.timer

echo "→ re-embed and re-score in the background (hours on a CPU box)"
systemd-run --unit=virtues-demo-reindex --collect /usr/local/bin/virtues reindex --yes

$PSQL -At -c "SELECT 'installed: ' || count(*) || ' rows; backbone ends ' || (SELECT landed FROM demo.buckets WHERE bucket = -1) || ', week ' || (SELECT min(landed) || '..' || max(landed) FROM demo.buckets WHERE bucket >= 0) FROM demo.rows"
