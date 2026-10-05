#!/usr/bin/env bash
# The Mac app's own copy of the web app.
#
# The Mac shows this copy for everything, at virtues://localhost, and uses its
# server for data only, the way the iPhone does (agents/plan/local-ui-plan.md).
# The server keeps it current over the air; this is the build the app ships
# with and falls back to.
#
# Out: apps/web/build-desktop/ (gitignored). Run from apps/web, which is where
# tauri runs its before-commands. `--if-missing` is for `tauri dev`, so a dev
# launch does not rebuild the whole app every time.
#
# THE VERSION STAMP. A release build stamps its copy with the nearest `v*` tag,
# as tools/ios-release.sh does for the iPhone. An unstamped copy reads `dev`,
# and a `dev` copy can never be ordered against the server's build, so it would
# refuse every over-the-air update. `tauri dev` stays `dev` on purpose: a
# server's copy must not replace the UI you are working on. An operator-set
# GIT_DESCRIBE wins.
set -euo pipefail

out=build-desktop
if [[ "${1:-}" == "--if-missing" ]]; then
	[[ -f "$out/index.html" ]] && exit 0
elif [[ -z "${GIT_DESCRIBE:-}" ]]; then
	GIT_DESCRIBE="$(git describe --tags --match 'v[0-9]*' --abbrev=0 2>/dev/null || echo dev)"
	if [[ "$GIT_DESCRIBE" == dev ]]; then
		echo "⚠ no v* tag reachable: the Mac's copy stamps as 'dev' and will take no over-the-air update" >&2
	fi
	export GIT_DESCRIBE
	export GIT_COMMIT="${GIT_COMMIT:-$(git rev-parse HEAD 2>/dev/null || echo dev)}"
fi

pnpm build
rm -rf "$out"
cp -R build "$out"
# SvelteKit's SPA fallback, answered for "/" too.
cp "$out/200.html" "$out/index.html"
# Precompressed siblings are for the server's static files, not this copy.
find "$out" -name '*.gz' -delete
# The connect page and its assets, which main.rs still opens for the
# VIRTUES_FORCE_CONNECT dev pin.
cp src-tauri/ui/connect.html src-tauri/ui/probe.html src-tauri/ui/jsqr.js src-tauri/ui/particles.js "$out/"
