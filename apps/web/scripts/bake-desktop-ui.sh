#!/usr/bin/env bash
# A native app's own copy of the web app: the Mac's, and with `--out
# build-ios`, the iPhone's.
#
# The app shows this copy for everything, at virtues://localhost, and uses its
# server for data only (agents/plan/local-ui-plan.md). The server keeps it
# current over the air; this is the build the app ships with and falls back to.
#
# Out: apps/web/build-desktop/ by default, or the `--out` directory (both
# gitignored). Run from apps/web, which is where tauri runs its
# before-commands. `--if-missing` is for `tauri dev`, so a dev launch does not
# rebuild the whole app every time.
#
# A copy, never `build/` itself: `build/` is what `make dev` serves, with its
# `.gz` siblings, and stripping those in place breaks every agent's dev server.
#
# THE VERSION STAMP. A release build stamps its copy with the nearest `v*` tag,
# as tools/ios-release.sh does for the iPhone. An unstamped copy reads `dev`,
# and a `dev` copy can never be ordered against the server's build, so it would
# refuse every over-the-air update. `tauri dev` stays `dev` on purpose: a
# server's copy must not replace the UI you are working on. An operator-set
# GIT_DESCRIBE wins.
set -euo pipefail

out=build-desktop
if_missing=0
while [[ $# -gt 0 ]]; do
	case "$1" in
		--if-missing) if_missing=1 ;;
		--out) out="${2:?--out needs a directory}"; shift ;;
		*) echo "usage: $0 [--if-missing] [--out DIR]" >&2; exit 2 ;;
	esac
	shift
done

if (( if_missing )); then
	[[ -f "$out/index.html" ]] && exit 0
elif [[ -z "${GIT_DESCRIBE:-}" ]]; then
	GIT_DESCRIBE="$(git describe --tags --match 'v[0-9]*' --abbrev=0 2>/dev/null || echo dev)"
	if [[ "$GIT_DESCRIBE" == dev ]]; then
		echo "⚠ no v* tag reachable: $out stamps as 'dev' and will take no over-the-air update" >&2
	fi
	export GIT_DESCRIBE
	export GIT_COMMIT="${GIT_COMMIT:-$(git rev-parse HEAD 2>/dev/null || echo dev)}"
fi

pnpm build
rm -rf "$out"
cp -R build "$out"
# Precompressed siblings are for the server's static files, not this copy.
find "$out" -name '*.gz' -delete
# The airlock, for the Mac's VIRTUES_FORCE_CONNECT dev pin only: main.rs opens
# it as `WebviewUrl::App`, which reads frontendDist rather than `serve_ui`.
# Every other load gets these from the copies compiled into the shell
# (src-tauri/src/lib.rs), so they sit outside the manifest's contentHash and
# never reach a bundle. The iPhone's bake does not need them.
# TODO(2026-10-05): migrate the dev pin in main.rs to `own_copy("connect.html#setup")`
# and delete this.
if [[ "$out" == build-desktop ]]; then
	cp src-tauri/ui/connect.html src-tauri/ui/probe.html src-tauri/ui/jsqr.js "$out/"
fi
