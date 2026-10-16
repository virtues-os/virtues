# Demo box health check, sourced by virtues-health (installed as
# /etc/virtues/health.d/demo.sh). Each line printed is a problem.
curl -sf --max-time 10 localhost:8000/health >/dev/null || echo "box /health not answering on :8000"
for u in virtues virtues-embed caddy postgresql; do systemctl is-active -q $u || echo "$u is not active"; done
for t in virtues-demo-place.timer virtues-demo-reset.timer; do systemctl is-active -q $t || echo "$t is not active"; done
# Today must be a full demo day: the placement is the whole point of the box.
n=$(sudo -u postgres psql -X -At -d virtues -c "SELECT count(*) FROM wiki_events e JOIN wiki_days d ON d.id = e.day_id WHERE d.date = current_date" 2>/dev/null)
[ "${n:-0}" -gt 0 ] || echo "today has no events: the demo life is not placed"
# The public doors, as App Review and visitors reach them.
c=$(curl -s -o /dev/null -w %{http_code} --max-time 10 https://demo-d5873b.virtues.ch/api/box/identity); [ "$c" = 200 ] || echo "review identity endpoint returned $c (want 200)"
c=$(curl -s -o /dev/null -w %{http_code} --max-time 10 https://demo.virtues.com/); [ "$c" = 401 ] || echo "demo.virtues.com returned $c without credentials (want 401: the gate is the only access control)"
c=$(curl -s -o /dev/null -w %{http_code} --max-time 10 https://demo-d5873b.virtues.ch/); [ "$c" = 401 ] || echo "review hostname root returned $c without credentials (want 401)"
true
