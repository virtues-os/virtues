#!/usr/bin/env python3
"""Package built background styles for servers to download.

Reads style folders that build.py wrote (each with a manifest.json and its
files) and writes, under --out:

  catalog.json             every style: its id, title, archive name, size,
                           sha256, and a small preview for Light and Dark
  <style>-<hash>.tar       one archive per style: manifest.json and every
                           file it names, nothing else

A server reads the catalog from virtues-api (<api>/v1/backgrounds/catalog.json)
and downloads an archive once, when its owner picks that style
(virtues-core/src/api/backgrounds.rs). The archive name carries the first 12
characters of its sha256, so a rebuilt style gets a new name and a server
never mixes an old download with a new one.

Upload the output folder to the maps bucket under backgrounds/, archives
first and catalog.json last, so a server never reads a catalog that names an
archive not uploaded yet.

    python3 tools/backgrounds/package.py \\
        --styles ~/virtues-backgrounds/watercolors ~/virtues-backgrounds/oil-paintings \\
        --out ~/virtues-backgrounds-dist

The archives are plain tar: the pictures are AVIF and WebP, which do not
compress further.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import io
import json
import sys
import tarfile
from pathlib import Path

from PIL import Image

PREVIEW_WIDTH = 480


def files_of(manifest: dict) -> list[str]:
    """Every file path the manifest names, relative to the style folder."""
    out = []
    for collection in manifest["collections"]:
        for picture in collection["pictures"]:
            for theme in ("light", "dark"):
                for fmt in picture[theme]["files"].values():
                    out.extend(fmt.values())
    return sorted(set(out))


def preview(style_dir: Path, manifest: dict, theme: str) -> str:
    """A small WebP of the style's first picture in one theme, as a data URL."""
    picture = manifest["collections"][0]["pictures"][0][theme]
    widths = sorted(picture["files"]["webp"], key=int)
    src = style_dir / picture["files"]["webp"][widths[0]]
    with Image.open(src) as im:
        im = im.convert("RGBA")
        h = round(im.height * PREVIEW_WIDTH / im.width)
        im = im.resize((PREVIEW_WIDTH, h), Image.LANCZOS)
        buf = io.BytesIO()
        im.save(buf, "WEBP", quality=70, method=6)
    return "data:image/webp;base64," + base64.b64encode(buf.getvalue()).decode("ascii")


def write_archive(style_dir: Path, manifest: dict, out_dir: Path) -> tuple[str, int, str]:
    """Write the style's archive and return (name, bytes, sha256). The entries
    have fixed times and owners, so the same files always give the same
    archive and the same name."""
    style = manifest["style"]
    tmp = out_dir / f".{style}.tar.tmp"
    names = ["manifest.json", *files_of(manifest)]
    with tarfile.open(tmp, "w", format=tarfile.PAX_FORMAT) as tar:
        for rel in names:
            path = style_dir / rel
            if not path.is_file():
                sys.exit(f"{style}: {rel} is named in the manifest but missing")
            info = tar.gettarinfo(str(path), arcname=rel)
            info.mtime = 0
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.mode = 0o644
            with path.open("rb") as f:
                tar.addfile(info, f)
    digest = hashlib.sha256(tmp.read_bytes()).hexdigest()
    name = f"{style}-{digest[:12]}.tar"
    final = out_dir / name
    tmp.replace(final)
    return name, final.stat().st_size, digest


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--styles", nargs="+", required=True, type=Path, help="style folders build.py wrote")
    ap.add_argument("--out", required=True, type=Path)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    entries = []
    for style_dir in args.styles:
        manifest = json.loads((style_dir / "manifest.json").read_text())
        if manifest.get("version") != 1:
            sys.exit(f"{style_dir}: manifest is not version 1")
        if manifest["style"] != style_dir.name:
            sys.exit(f"{style_dir}: the manifest names {manifest['style']}, not the folder's name")
        name, size, digest = write_archive(style_dir, manifest, args.out)
        entries.append(
            {
                "style": manifest["style"],
                "title": manifest["title"],
                "renderer": manifest["renderer"],
                "archive": name,
                "bytes": size,
                "sha256": digest,
                "preview": {theme: preview(style_dir, manifest, theme) for theme in ("light", "dark")},
            }
        )
        print(f"{name}: {size / 1e6:.1f} MB")

    keep = {e["archive"] for e in entries}
    for old in args.out.glob("*.tar"):
        if old.name not in keep:
            old.unlink()
            print(f"removed {old.name}")
    catalog = {"version": 1, "styles": sorted(entries, key=lambda e: e["style"])}
    tmp = args.out / ".catalog.json.tmp"
    tmp.write_text(json.dumps(catalog, indent=1) + "\n")
    tmp.replace(args.out / "catalog.json")
    print(f"wrote {args.out / 'catalog.json'}: {len(entries)} styles")


if __name__ == "__main__":
    main()
