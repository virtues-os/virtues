#!/usr/bin/env bash
#
# build-icons.sh — regenerate every icon artifact from the one source of truth,
# apps/web/src-tauri/icons/AppIcon.icon.
#
# Run this after changing the mark. Nothing else regenerates these; the outputs
# are committed, because the places that consume them cannot compile them:
#
#   * Release CI runs `pnpm tauri build` directly on a macos-15 runner, not
#     tools/build-mac-app.sh — a pre-build compile step there would silently
#     not run in the one build that ships to users.
#   * Listing a `.icon` in `bundle.icon` switches tauri onto an actool path
#     that does not work with this toolchain and takes the whole bundle step
#     with it. That broke `make mac-app` for every agent for half a day
#     (6e99803d, then d9752668). We compile actool's output ourselves and hand
#     tauri two ordinary files instead.
#
# Produces:
#   icons/AppIcon.compiled/{Assets.car,AppIcon.icns}
#       The layered icon — light / dark / clear / tinted. Copied into
#       Contents/Resources by `bundle.resources`, addressed by CFBundleIconName
#       in src-tauri/Info.plist. This is the macOS 26 path.
#   icons/master-1024.png
#       Full-bleed square render of the same mark, no squircle corners. This is
#       the input to `tauri icon`, which derives the flat sets for iOS, Android,
#       Windows and Linux. It must be square and full-bleed: iOS applies its own
#       mask, so a pre-rounded source gets visibly double-rounded.
#   everything `tauri icon` derives from that master.
#
# Requires Xcode 26 (actool + Icon Composer's ictool). Both ship inside
# Xcode.app; neither is on PATH by default.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ICONS="$REPO_ROOT/apps/web/src-tauri/icons"
SRC="$ICONS/AppIcon.icon"
COMPILED="$ICONS/AppIcon.compiled"
MASTER="$ICONS/master-1024.png"

say() { printf '∴ %s\n' "$*"; }
die() { printf '✖ %s\n' "$*" >&2; exit 1; }

[[ -d "$SRC" ]] || die "no $SRC"

# ── 1. compile the layered icon ─────────────────────────────────────────────
# actool delegates to a long-lived `ibtoold` helper that WEDGES once it has hit
# a bad input, and then fails on valid input too — which makes a correct icon
# look broken. Three builds were lost to that before someone restarted it.
pkill -f ibtoold >/dev/null 2>&1 || true

say "compiling AppIcon.icon (actool)"
rm -rf "$COMPILED" && mkdir -p "$COMPILED"
xcrun actool "$SRC" \
  --compile "$COMPILED" \
  --app-icon AppIcon \
  --output-partial-info-plist "$COMPILED/partial.plist" \
  --platform macosx \
  --minimum-deployment-target 26.0 \
  --output-format human-readable-text >/dev/null
# The partial plist only restates CFBundleIconName/CFBundleIconFile, which we
# carry explicitly in src-tauri/Info.plist. Drop it so it can't drift.
rm -f "$COMPILED/partial.plist"
[[ -f "$COMPILED/Assets.car" && -f "$COMPILED/AppIcon.icns" ]] \
  || die "actool produced no Assets.car/AppIcon.icns"
say "  $(du -h "$COMPILED/Assets.car" | cut -f1) Assets.car, $(du -h "$COMPILED/AppIcon.icns" | cut -f1) AppIcon.icns"

# ── 2. full-bleed square master for the flat platforms ──────────────────────
# Deliberately NOT ictool's export: every ictool rendition bakes in the squircle
# and its transparent margin, which is right for macOS and wrong for everyone
# else. The geometry here is the same mark, and must stay in step with
# icons/source/mark.svg — see that file's header for the derivation.
say "rendering the full-bleed master"
tmp_svg="$(mktemp -t virtues-icon).svg"
cat > "$tmp_svg" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#17364f"/>
      <stop offset="1" stop-color="#0c1e2e"/>
    </linearGradient>
  </defs>
  <rect width="1024" height="1024" fill="url(#g)"/>
  <g fill="#f8fff7">
    <circle cx="512" cy="338" r="73"/>
    <circle cx="329" cy="655" r="73"/>
    <circle cx="695" cy="655" r="73"/>
  </g>
</svg>
SVG
rm -f "$MASTER"
qlmanage -t -s 1024 -o "$(dirname "$tmp_svg")" "$tmp_svg" >/dev/null 2>&1 || true
[[ -f "$tmp_svg.png" ]] || die "qlmanage produced no PNG from $tmp_svg"
# qlmanage always writes a square canvas at the requested size; ours is already
# square, so this is a straight 1024x1024.
mv "$tmp_svg.png" "$MASTER"
rm -f "$tmp_svg"
say "  $(sips -g pixelWidth -g pixelHeight "$MASTER" | tail -2 | tr -d ' \n')"

# ── 3. derive the flat sets ─────────────────────────────────────────────────
TAURI_BIN="$REPO_ROOT/apps/web/node_modules/.bin/tauri"
if [[ -x "$TAURI_BIN" ]]; then
  say "deriving flat icon sets (tauri icon)"
  ( cd "$REPO_ROOT/apps/web" && "$TAURI_BIN" icon "$MASTER" >/dev/null )
  # `tauri icon` writes RGBA into the iOS appiconset, and App Store validation
  # rejects an alpha channel on the large app icon even when it is fully opaque
  # — ours measured alpha 254-255 everywhere and still 409'd. Flatten now so a
  # release build never has to remember to.
  say "flattening iOS icons to RGB"
  python3 "$REPO_ROOT/tools/strip-icon-alpha.py"
else
  printf '⚠ tauri CLI not found — skipped the flat sets. Run pnpm install in apps/web.\n'
fi

printf '\n'
say "icons rebuilt. Review with: git status apps/web/src-tauri/icons"
