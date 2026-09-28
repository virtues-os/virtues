#!/usr/bin/env bash
# The Mac app's own copy of the web app, for Setup's first half.
#
# Before a Mac has a server, Setup (sign in, find the server, its four words,
# Wi-Fi, pair) runs from this copy at tauri://localhost/setup; after pairing
# the window hands over to the server's own copy, as it always has. The
# compiled-in connect page and its assets ride along beside it, because the
# shell still opens them for the recovery screens (#reset, #unreachable).
#
# Out: apps/web/build-desktop/ (gitignored). Run from apps/web, which is where
# tauri runs its before-commands. `--if-missing` is for `tauri dev`, so a dev
# launch does not rebuild the whole app every time.
set -euo pipefail

out=build-desktop
if [[ "${1:-}" == "--if-missing" && -f "$out/index.html" ]]; then
	exit 0
fi

pnpm build
rm -rf "$out"
cp -R build "$out"
# SvelteKit's SPA fallback, answered for "/" too (tauri serves index.html for
# any path it has no file for).
cp "$out/200.html" "$out/index.html"
# The shell serves these itself; precompressed siblings are for the server.
find "$out" -name '*.gz' -delete
# The connect page and its assets, at the paths main.rs opens.
cp src-tauri/ui/connect.html src-tauri/ui/probe.html src-tauri/ui/jsqr.js src-tauri/ui/particles.js "$out/"
