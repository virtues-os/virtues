#!/bin/sh
# Stage the loader for serving: dist/ holds index.html plus the wasm build
# under wasm/<version>/, with index.html importing that exact version.
#
# index.html is served uncached and the wasm files are cached, so a fixed
# path would let a fresh index.html meet yesterday's cached script. A path
# per build makes each index.html name the only files it works with.
#
#     sh apps/loader/stage.sh      # after the build in README.md
set -eu
cd "$(dirname "$0")"
[ -f public/wasm/virtues_loader.js ] || { echo "build first (README.md)"; exit 1; }
VERSION=$(cat public/wasm/virtues_loader.js public/wasm/virtues_loader_bg.wasm | shasum -a 256 | cut -c1-12)
rm -rf dist
mkdir -p "dist/wasm/$VERSION"
cp public/wasm/virtues_loader.js public/wasm/virtues_loader_bg.wasm "dist/wasm/$VERSION/"
sed "s#\./wasm/virtues_loader\.js#./wasm/$VERSION/virtues_loader.js#" public/index.html > dist/index.html
grep -q "wasm/$VERSION/virtues_loader.js" dist/index.html || { echo "index.html import not rewritten"; exit 1; }
echo "staged dist/ (wasm/$VERSION)"
(cd dist && shasum -a 256 index.html "wasm/$VERSION/virtues_loader.js" "wasm/$VERSION/virtues_loader_bg.wasm")
