#!/usr/bin/env python3
"""Flatten the iOS app-icon set to RGB.

App Store validation rejects an alpha channel on the app icon — not just
transparency, the channel itself. `tauri icon` writes RGBA, so every run
reintroduces the fault and the next Transporter upload dies at validation:

    Validation failed (409) — Invalid large app icon. The large app icon in
    the asset catalog in "Virtues.app" can't be transparent or contain an
    alpha channel.

Ours measured alpha 254-255 across every icon (a handful of antialiased edge
pixels below full) and still failed, so "it looks opaque" is not a defence.

Compositing onto white is safe precisely because the icons are already
opaque — the recorded max per-channel delta is 1. If a future icon has real
transparency, change the background here deliberately rather than letting
white decide it.

Run after any `tauri icon`, before archiving.
"""

import glob
import os
import sys

try:
    from PIL import Image
except ImportError:
    sys.exit("Pillow required: pip install Pillow")

APPICONSET = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "apps/web/src-tauri/gen/apple/Assets.xcassets/AppIcon.appiconset",
)
BACKGROUND = (255, 255, 255)


def main() -> int:
    paths = sorted(glob.glob(os.path.join(APPICONSET, "*.png")))
    if not paths:
        sys.exit(f"no icons found at {APPICONSET}")

    flattened = 0
    for path in paths:
        image = Image.open(path)
        if image.mode != "RGBA":
            continue
        ground = Image.new("RGB", image.size, BACKGROUND)
        ground.paste(image, mask=image.getchannel("A"))
        ground.save(path, format="PNG")
        flattened += 1
        print(f"  flattened {os.path.basename(path)}")

    print(f"∴ {flattened} of {len(paths)} icons flattened to RGB")

    still_rgba = [
        os.path.basename(p) for p in paths if Image.open(p).mode == "RGBA"
    ]
    if still_rgba:
        sys.exit(f"still RGBA after flatten: {', '.join(still_rgba)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
