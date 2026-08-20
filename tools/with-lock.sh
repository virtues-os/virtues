#!/bin/sh
# Run a command while holding the repo's git lock.
#
# Several agents share this checkout, so the git index is a shared mutable
# resource — two agents interleaving `git add` and `git commit` means whoever
# commits first takes both sets of changes. Callers that touch the index (see
# `make commit` / `make migration`) serialize through here.
#
# `flock` is Linux-only; this uses a mkdir spinlock, which is atomic on every
# POSIX filesystem and works on macOS.
#
#   tools/with-lock.sh <command> [args...]

set -eu

LOCK_DIR="${VIRTUES_LOCK_DIR:-.git/virtues.lock}"
TIMEOUT="${VIRTUES_LOCK_TIMEOUT:-120}"

[ $# -gt 0 ] || { echo "with-lock: nothing to run" >&2; exit 2; }

waited=0
until mkdir "$LOCK_DIR" 2>/dev/null; do
    # Reap a lock whose owner died without releasing it.
    if [ -f "$LOCK_DIR/pid" ]; then
        owner=$(cat "$LOCK_DIR/pid" 2>/dev/null || echo "")
        if [ -n "$owner" ] && ! kill -0 "$owner" 2>/dev/null; then
            echo "with-lock: clearing stale lock from dead pid $owner" >&2
            rm -rf "$LOCK_DIR"
            continue
        fi
    fi
    waited=$((waited + 1))
    if [ "$waited" -ge "$TIMEOUT" ]; then
        echo "with-lock: timed out after ${TIMEOUT}s waiting for $LOCK_DIR" >&2
        echo "  another agent may be mid-commit; if nothing is running, rm -rf $LOCK_DIR" >&2
        exit 1
    fi
    [ "$waited" = 5 ] && echo "with-lock: waiting for another agent to finish committing..." >&2
    sleep 1
done

echo $$ > "$LOCK_DIR/pid"
trap 'rm -rf "$LOCK_DIR"' EXIT INT TERM

"$@"
