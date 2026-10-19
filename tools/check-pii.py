#!/usr/bin/env python3
"""No real contact details, and no raw exports of a life, in what this repo commits.

The repo is public, and real contact details have reached it in code comments,
test constants, fixtures, seeds and commit messages. Fiction has a reserved
block, so anything outside it is refused:

    phones   +1 512 555 xxxx only
    emails   example.com / virtues.com (and their subdomains), plus the
             reserved example.org/.net and .example/.test/.invalid names

A legitimate service address (a vendor's support inbox, a crate author's
contact in vendored code) goes in tools/pii-allowlist.txt, one per line, or as
`@domain` for a whole domain. Keep that file short; every line is a claim that
the address belongs to no private person. A commit's author and committer
address must be a GitHub noreply address, never a personal mailbox.

The second half refuses the FILES a life gets exported as, whatever they are
named: a voice memo, a phone photo carrying GPS, a HealthKit export, a
location trace, a notebook with its outputs, a database, an archive. Each of
those has reached this repo. Types are told by their bytes, not by their
extension, so renaming one does not get it through. Refused:

    audio, video, HEIC                      any image whose EXIF/XMP carries GPS
    GPX, KML, Health export.xml, CDA        CSV/TSV, and tabular .txt/.dat/.log
    SQLite, DuckDB, Parquet, Arrow, Avro    vCard, iCalendar, mbox, .eml
    zip (docx, xlsx, kmz), tar, bplist      gzip/zstd/xz/bz2/7z/rar outside GZIP_OK
    any Jupyter notebook                    a new file over 500 KB not in LARGE_OK

An exception is an exact path in GZIP_OK, LARGE_OK or TYPE_OK below, added in
a reviewed change with the reason beside it.

    tools/check-pii.py [REV]                   the committed tree (default HEAD)
    tools/check-pii.py --staged                the commit being made (pre-commit)
    tools/check-pii.py --message FILE          a commit message (commit-msg)
    tools/check-pii.py --range A..B [--ref R]  every commit in A..B: added lines,
                                               files, messages and authors (CI)
    tools/check-pii.py --push                  pre-push: the ranges git gives on stdin
    ... --large-kb N                           a different new-file size limit

The same file runs in the website repo (tools/check-pii.py there is a copy):
the site is public even though that repo is not.

Tree mode reads the COMMITTED tree (`git ls-tree`), never the working tree: in
the shared checkout the working tree holds every agent's in-flight edits, and
CI reads the commit. Gzipped files are decompressed and read like any other.

tools/privacy-guard.py is the other half: it knows the owner's real values (by
hash) and catches a real number inside the fictional shape, or message text,
which no pattern can. This half needs no real data, so it runs anywhere,
including CI.
"""

from __future__ import annotations

import argparse
import gzip
import re
import struct
import subprocess
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ALLOWLIST = ROOT / "tools/pii-allowlist.txt"

FICTIONAL_DOMAINS = ("example.com", "virtues.com", "example.org", "example.net")
RESERVED_TLDS = (".example", ".test", ".invalid", ".localhost", ".local")
# Addresses that exist so that no person's mailbox is published: GitHub's
# per-account noreply form, its web-flow committer, and the co-author trailer.
NOREPLY_DOMAINS = ("users.noreply.github.com",)
NOREPLY = {"noreply@github.com", "noreply@anthropic.com"}

# A phone number with a phone's punctuation: +E.164, (512) 555-0101, 512-555-0101,
# 512.555.0101. A bare run of ten digits is a timestamp or an id far more often
# than a number, so it is not matched.
PHONE = re.compile(
    r"(?<![\w.+/-])(?:"
    r"\+\d{10,14}"
    r"|\+?1?[ .-]?\(\d{3}\)[ .-]?\d{3}[ .-]\d{4}"
    r"|(?:\+?1[ .-])?\d{3}([.-])\d{3}\1\d{4}"
    r")(?![\w-])"
)
EMAIL = re.compile(r"(?<![\w.%+-])[A-Za-z0-9][A-Za-z0-9._%+-]*@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,})\b")

# Files whose bytes are not prose anyone wrote here. THIRD_PARTY_NOTICES carries
# upstream authors' published licence contacts verbatim.
SKIP = re.compile(r"(^|/)(pnpm-lock\.yaml|Cargo\.lock|package-lock\.json|THIRD_PARTY_NOTICES\.txt)$"
                  r"|\.(min\.js|map|svg)$")
# `icon@2x.png`, `pkg@3.1.0.patch`: an asset or a package spec, not a mailbox.
NOT_A_TLD = {"png", "jpg", "jpeg", "gif", "webp", "svg", "pdf", "patch", "js", "ts", "json", "css", "rs", "md"}

# ── the file gate ─────────────────────────────────────────────────────────

LARGE = 500 * 1024
# The repo ships no compressed files: a .gz is how a data dump arrives.
GZIP_OK: set[str] = set()
LARGE_OK = GZIP_OK | {
    "apps/web/src-tauri/icons/AppIcon.compiled/Assets.car",  # compiled app icon
    "apps/web/static/templates/morning-examen.png",          # template artwork
    ".github/images/cover3.png",                             # README cover
}
# path -> why a file of a refused type is committed on purpose.
TYPE_OK: dict[str, str] = {}

TABULAR_EXT = re.compile(r"\.(csv|tsv|psv)$", re.I)
# Extensions a raw export hides behind; their content is sniffed for columns.
PLAIN_EXT = re.compile(r"(^|/)[^/.]+$|\.(txt|dat|log|out|tab)$", re.I)
VIDEO_OR_AUDIO_FTYP = {b"M4A ": "audio (m4a)", b"M4B ": "audio (m4b)", b"M4P ": "audio (m4p)"}
HEIC_FTYP = {b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs", b"mif1", b"msf1"}
EMAIL_HEADERS = re.compile(rb"^(?:From|To|Date|Subject|Message-ID|Received|Return-Path|Delivered-To|MIME-Version):",
                           re.I | re.M)


def tiff_has_gps(tiff: bytes) -> bool:
    """True when a TIFF/EXIF block's first IFD points at a GPS IFD with entries."""
    try:
        e = {b"II": "<", b"MM": ">"}[tiff[:2]]
        off = struct.unpack(e + "I", tiff[4:8])[0]
        n = struct.unpack(e + "H", tiff[off:off + 2])[0]
        for i in range(n):
            ent = tiff[off + 2 + 12 * i:off + 14 + 12 * i]
            if struct.unpack(e + "H", ent[:2])[0] == 0x8825:
                gps = struct.unpack(e + "I", ent[8:12])[0]
                return struct.unpack(e + "H", tiff[gps:gps + 2])[0] > 0
    except (KeyError, struct.error, IndexError):
        pass
    return False


def image_has_gps(data: bytes) -> bool:
    if b"GPSLatitude" in data[:1 << 20]:  # XMP, in any container
        return True
    if data[:3] == b"\xff\xd8\xff":  # JPEG: walk the segments to APP1 Exif
        i = 2
        while i + 4 <= len(data) and data[i] == 0xFF:
            marker, length = data[i + 1], struct.unpack(">H", data[i + 2:i + 4])[0]
            if marker == 0xE1 and data[i + 4:i + 10] == b"Exif\0\0" and tiff_has_gps(data[i + 10:i + 2 + length]):
                return True
            if marker == 0xDA:
                break
            i += 2 + length
        return False
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        i = 8
        while i + 8 <= len(data):
            length, kind = struct.unpack(">I", data[i:i + 4])[0], data[i + 4:i + 8]
            body = data[i + 8:i + 8 + length]
            if kind == b"eXIf" and tiff_has_gps(body):
                return True
            if kind in (b"zTXt", b"iTXt") and b"exif" in body[:80].lower():
                try:
                    if b"GPS" in zlib.decompress(body[body.index(b"\0") + 2:]):
                        return True
                except (ValueError, zlib.error):
                    pass
            i += 12 + length
        return False
    if data[:4] == b"RIFF" and data[8:12] == b"WEBP":
        i = 12
        while i + 8 <= len(data):
            kind, length = data[i:i + 4], struct.unpack("<I", data[i + 4:i + 8])[0]
            body = data[i + 8:i + 8 + length]
            if kind == b"EXIF" and tiff_has_gps(body[6:] if body.startswith(b"Exif\0\0") else body):
                return True
            i += 8 + length + (length & 1)
        return False
    if data[:4] in (b"II*\0", b"MM\0*"):
        return tiff_has_gps(data)
    return False


def sniff(path: str, data: bytes) -> str | None:
    """What kind of refused file these bytes are, or None."""
    head = data[:512]
    if head[4:8] == b"ftyp":
        brand = head[8:12]
        if brand in HEIC_FTYP:
            return "HEIC/HEIF photo"
        if brand in (b"avif", b"avis"):
            return "a photo carrying GPS" if image_has_gps(data) else None
        return VIDEO_OR_AUDIO_FTYP.get(brand, "video or audio (MP4/MOV family)")
    if head[:3] == b"ID3" or (head[:1] == b"\xff" and head[1:2] in (b"\xfb", b"\xf3", b"\xf2", b"\xfa", b"\xe3", b"\xf1", b"\xf9")):
        return "audio (MP3/AAC)"
    if head[:4] == b"RIFF":
        return {b"WAVE": "audio (WAV)", b"AVI ": "video (AVI)"}.get(head[8:12]) or \
            ("a photo carrying GPS" if image_has_gps(data) else None)
    for magic, kind in ((b"FORM", "audio (AIFF)"), (b"fLaC", "audio (FLAC)"), (b"OggS", "audio (Ogg/Opus)"),
                        (b"caff", "audio (CAF)"), (b"#!AMR", "audio (AMR)"), (b"\x1aE\xdf\xa3", "video (WebM/MKV)"),
                        (b"0&\xb2u", "audio/video (ASF/WMA)"),
                        (b"SQLite format 3\0", "SQLite database"), (b"PAR1", "Parquet"), (b"ARROW1", "Arrow"),
                        (b"Obj\x01", "Avro"), (b"PK\x03\x04", "zip archive (docx/xlsx/kmz/...)"),
                        (b"PK\x05\x06", "zip archive"), (b"bplist00", "binary plist"),
                        (b"\x28\xb5\x2f\xfd", "zstd archive"), (b"BZh", "bzip2 archive"),
                        (b"\xfd7zXZ\0", "xz archive"), (b"7z\xbc\xaf\x27\x1c", "7z archive"), (b"Rar!", "rar archive")):
        if head.startswith(magic):
            return kind
    if head[8:12] == b"DUCK":
        return "DuckDB database"
    if data[257:262] == b"ustar":
        return "tar archive"
    if head[:2] == b"\x1f\x8b":
        return None if path in GZIP_OK else "gzip archive (the repo ships none)"
    if head[:3] == b"\xff\xd8\xff" or head[:8] == b"\x89PNG\r\n\x1a\n" or head[:4] in (b"II*\0", b"MM\0*"):
        return "a photo carrying GPS" if image_has_gps(data) else None
    if b"\0" in data[:8000]:
        return None

    text = data[:65536].lstrip(b"\xef\xbb\xbf \t\r\n")
    low = text[:4096].lower()
    if b"<gpx" in low:
        return "GPX location trace"
    if b"<kml" in low:
        return "KML location file"
    if b"<healthdata" in low or b"<!doctype healthdata" in low:
        return "Apple Health export"
    if b"<clinicaldocument" in low:
        return "health record (CDA)"
    if text.startswith(b"{") and b'"nbformat"' in text and b'"cells"' in text:
        return "Jupyter notebook"
    if low.startswith(b"begin:vcard"):
        return "vCard contacts"
    if low.startswith(b"begin:vcalendar"):
        return "iCalendar export"
    if re.match(rb"From \S+ +\w{3} \w{3} +\d", text):
        return "mbox mail archive"
    if EMAIL_HEADERS.match(text) and len(EMAIL_HEADERS.findall(text[:8192])) >= 3:
        return "email message (.eml)"
    if TABULAR_EXT.search(path):
        return "CSV/TSV table"
    if PLAIN_EXT.search(path) and is_tabular(text):
        return "a table (CSV/TSV under another name)"
    return None


def is_tabular(text: bytes) -> bool:
    lines = text.decode("utf-8", "replace").splitlines()[:200]
    if len(lines) < 20:
        return False
    for sep in (",", "\t", ";", "|"):
        counts = [ln.count(sep) for ln in lines if ln.strip()]
        common = max(set(counts), key=counts.count)
        if common >= 2 and counts.count(common) >= 0.8 * len(counts):
            return True
    return False


def gate(path: str, data: bytes, size: int, grew_past: bool) -> str | None:
    """Why this file may not be committed, or None. `grew_past`: it is new, or just crossed LARGE."""
    if path in TYPE_OK:
        return None
    kind = sniff(path, data)
    if kind:
        return kind
    if size > LARGE and grew_past and path not in LARGE_OK:
        return f"{size // 1024} KB, over the {LARGE // 1024} KB limit for a new file"
    return None


# ── contact details ───────────────────────────────────────────────────────


def fictional_phone(raw: str) -> bool:
    digits = re.sub(r"\D", "", raw)
    if len(digits) == 10:
        digits = "1" + digits
    return digits.startswith("1512555") and len(digits) == 11


def fictional_email(domain: str) -> bool:
    d = domain.lower()
    return any(d == f or d.endswith("." + f) for f in FICTIONAL_DOMAINS) or d.endswith(RESERVED_TLDS)


def load_allowlist() -> tuple[set[str], set[str]]:
    exact, domains = set(NOREPLY), set(NOREPLY_DOMAINS)
    if ALLOWLIST.exists():
        for line in ALLOWLIST.read_text().splitlines():
            line = line.split("#", 1)[0].strip().lower()
            if line.startswith("@"):
                domains.add(line[1:])
            elif line:
                exact.add(line)
    return exact, domains


ALLOWED = load_allowlist()


def address_ok(addr: str) -> bool:
    addr = addr.lower()
    domain = addr.rsplit("@", 1)[-1]
    exact, domains = ALLOWED
    return fictional_email(domain) or addr in exact or domain in domains or \
        any(domain.endswith("." + d) for d in NOREPLY_DOMAINS)


def contact_hits(lines):
    """(line, kind, value) for each phone or email outside the fictional blocks."""
    exact, _ = ALLOWED
    for no, line in lines:
        for m in PHONE.finditer(line):
            if not fictional_phone(m.group(0)) and m.group(0) not in exact:
                yield no, "phone", m.group(0)
        for m in EMAIL.finditer(line):
            if m.group(1).rsplit(".", 1)[1].lower() in NOT_A_TLD:
                continue
            if not address_ok(m.group(0)):
                yield no, "email", m.group(0)


# ── reading git ───────────────────────────────────────────────────────────

ZERO = "0" * 40
EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"


def git(*args: str, input: bytes | None = None, check: bool = True) -> bytes:
    r = subprocess.run(["git", "-c", "core.quotepath=off", *args],
                       input=input, capture_output=True)
    if check and r.returncode != 0:
        sys.exit(f"check-pii: git {' '.join(args[:3])} failed: {r.stderr.decode(errors='replace').strip()}")
    return r.stdout if r.returncode == 0 else b""


def blobs(specs: list[str]) -> list[bytes | None]:
    """Contents for each `rev:path` or blob id, through one cat-file process."""
    if not specs:
        return []
    out = git("cat-file", "--batch", input="".join(s + "\n" for s in specs).encode())
    res, pos = [], 0
    for _ in specs:
        end = out.index(b"\n", pos)
        header = out[pos:end].split()
        if len(header) != 3:
            res.append(None)
            pos = end + 1
            continue
        size = int(header[2])
        res.append(out[end + 1:end + 1 + size])
        pos = end + 1 + size + 1
    return res


def text_lines(path: str, data: bytes | None) -> list[str] | None:
    """The file as text lines, a .gz decompressed; None for anything binary."""
    if data is None:
        return None
    if path.endswith(".gz"):
        try:
            data = gzip.decompress(data)
        except (OSError, EOFError):
            return None
    elif b"\0" in data[:8000]:
        return None
    return data.decode("utf-8", "replace").splitlines()


class Report:
    def __init__(self):
        self.contacts: list[tuple[str, int, str, str]] = []
        self.files: list[tuple[str, str]] = []

    def scan_lines(self, path: str, lines, label: str = ""):
        if not SKIP.search(path) and path != "tools/pii-allowlist.txt":
            self.contacts.extend((label + path, no, kind, v) for no, kind, v in contact_hits(lines))

    def emit(self) -> int:
        for where, no, kind, value in self.contacts:
            print(f"{where}{f':{no}' if no else ''}: {kind} outside the fictional block: {value}")
        for where, why in self.files:
            print(f"{where}: {why}")
        if self.contacts:
            print(f"""
✖ {len(self.contacts)} contact detail(s) outside the fictional blocks.
  Use +1512555xxxx and @example.com. If one is a real SERVICE address (no private
  person behind it), add it to tools/pii-allowlist.txt. A commit's author must be
  your GitHub noreply address: git config user.email <id>+<login>@users.noreply.github.com""",
                  file=sys.stderr)
        if self.files:
            print(f"""
✖ {len(self.files)} file(s) of a kind a life gets exported as.
  Raw exports, recordings, photos with GPS and databases never belong in this
  repo, and neither does demo data. A file that is genuinely needed goes in
  GZIP_OK, LARGE_OK or TYPE_OK in tools/check-pii.py, by exact path, with a reason.""",
                  file=sys.stderr)
        return 1 if self.contacts or self.files else 0


def scan_tree(rev: str, rep: Report):
    raw = git("ls-tree", "-r", "-z", "-l", rev).decode("utf-8", "replace")
    entries = []
    for rec in filter(None, raw.split("\0")):
        meta, path = rec.split("\t", 1)
        mode, kind, sha, size = meta.split()
        if kind == "blob" and mode != "120000":
            entries.append((path, sha, int(size)))
    for (path, _, size), data in zip(entries, blobs([sha for _, sha, _ in entries])):
        if data is None:
            continue
        why = gate(path, data, size, grew_past=True)
        if why:
            rep.files.append((path, why))
        lines = text_lines(path, data)
        if lines is not None:
            rep.scan_lines(path, enumerate(lines, 1))


def diff_paths(base: str, target: str | None, only: set[str] | None = None):
    """Changed files between base and target (None = the index): (status, old, new, path)."""
    args = ["diff", "--raw", "-z", "--no-abbrev", "-M", "--no-ext-diff"]
    args += ["--cached", base] if target is None else [base, target]
    fields = git(*args).decode("utf-8", "replace").split("\0")
    i = 0
    while i < len(fields) - 1:
        meta = fields[i].split()
        if not meta:
            break
        status = meta[4][0]
        path = fields[i + 2] if status in "RC" else fields[i + 1]
        i += 3 if status in "RC" else 2
        if status != "D" and (only is None or path in only):
            yield status, meta[2], meta[3], path


def changed_files(base: str, target: str | None, only: set[str] | None = None):
    """(path, data, before, new) for each file a change adds or modifies.

    `before` is the old content when the path existed (None otherwise); `new`
    says the path is new, or just crossed the size limit. The same reader serves
    this script and tools/privacy-guard.py, for the index and for commits."""
    changed = [c for c in diff_paths(base, target, only) if c[2] != ZERO]
    new_data = blobs([new for _, _, new, _ in changed])
    old_iter = iter(blobs([old for status, old, _, _ in changed if status in "MT" and old != ZERO]))
    for (status, old, _, path), data in zip(changed, new_data):
        if data is None:
            continue
        before = next(old_iter) if status in "MT" and old != ZERO else None
        yield path, data, before, status not in "MT" or (before is not None and len(before) <= LARGE)


def added_lines(path: str, data: bytes, before: bytes | None) -> list[tuple[int, str]] | None:
    """The lines this change adds, numbered. A line moved within the file is not new."""
    lines = text_lines(path, data)
    if lines is None:
        return None
    old = set(text_lines(path, before) or []) if before is not None else set()
    return [(n, ln) for n, ln in enumerate(lines, 1) if ln not in old]


def scan_diff(base: str, target: str | None, rep: Report, label: str = "", only: set[str] | None = None):
    for path, data, before, new in changed_files(base, target, only):
        why = gate(path, data, len(data), new)
        if why:
            rep.files.append((label + path, why))
        lines = added_lines(path, data, before)
        if lines is not None:
            rep.scan_lines(path, lines, label)


def head_or_empty() -> str:
    """HEAD, or the empty tree in a repository with no commit yet."""
    r = subprocess.run(["git", "rev-parse", "-q", "--verify", "HEAD"], capture_output=True)
    return "HEAD" if r.returncode == 0 else EMPTY_TREE


def commit_range(spec: str, pushed_ref: str | None = None) -> list[str]:
    """Commits in A..B, oldest first. With A unknown (a new branch, a force push
    whose old tip was never fetched), every commit no remote branch already has.

    In CI the remote-tracking ref of the branch just pushed already points at B,
    so `pushed_ref` names it to leave it out of that comparison."""
    a, _, b = spec.partition("..")
    known = a and a != ZERO and subprocess.run(
        ["git", "cat-file", "-e", f"{a}^{{commit}}"], capture_output=True).returncode == 0
    if known:
        args = [f"{a}..{b}"]
    else:
        args = [b, "--not"]
        if pushed_ref and pushed_ref.startswith("refs/heads/"):
            args.append(f"--exclude=*/{pushed_ref[len('refs/heads/'):]}")
        args.append("--remotes")
    return git("rev-list", "--reverse", "--topo-order", *args).decode().split()


def scan_commits(commits: list[str], rep: Report, authors: bool = True):
    for c in commits:
        info = git("show", "-s", "--format=%P%x00%ae%x00%ce%x00%B", c).decode("utf-8", "replace")
        parents, author, committer, message = info.split("\0", 3)
        short = c[:9]
        rep.scan_lines("(message)", enumerate(message.splitlines(), 1), f"{short} ")
        for role, addr in (("author", author), ("committer", committer)):
            if authors and addr and not address_ok(addr):
                rep.contacts.append((f"{short} ({role})", 0, "email", addr))
        parents = parents.split()
        if len(parents) > 1:
            # A merge adds nothing of its own except where it differs from every parent.
            evil = set(filter(None, git("diff-tree", "--cc", "--name-only", "-z", "--no-commit-id", c)
                              .decode("utf-8", "replace").split("\0")))
            if evil:
                scan_diff(parents[0], c, rep, f"{short}:", only=evil)
        else:
            scan_diff(parents[0] if parents else EMPTY_TREE, c, rep, f"{short}:")


def main() -> int:
    global LARGE
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("rev", nargs="?", default="HEAD")
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--staged", action="store_true")
    mode.add_argument("--message", metavar="FILE")
    mode.add_argument("--range", metavar="A..B")
    mode.add_argument("--push", action="store_true")
    ap.add_argument("--ref", help="with --range: the ref that was pushed")
    ap.add_argument("--no-author-check", action="store_true",
                    help="skip commit author addresses (a private repo, whose commits are not published)")
    ap.add_argument("--large-kb", type=int, default=LARGE // 1024,
                    help="the new-file size limit (the website, which ships artwork, raises it)")
    args = ap.parse_args()
    LARGE = args.large_kb * 1024

    rep = Report()
    if args.staged:
        scan_diff(head_or_empty(), None, rep)
    elif args.message:
        text = Path(args.message).read_text(errors="replace")
        lines = [ln for ln in text.splitlines() if not ln.startswith("#")]
        rep.scan_lines("commit message", enumerate(lines, 1))
    elif args.range:
        scan_commits(commit_range(args.range, args.ref), rep, not args.no_author_check)
    elif args.push:
        for line in sys.stdin.read().splitlines():
            local_ref, local, remote_ref, remote = (line.split() + ["", "", "", ""])[:4]
            if not local or local == ZERO:
                continue  # a delete pushes nothing
            scan_commits(commit_range(f"{remote}..{local}"), rep, not args.no_author_check)
    else:
        scan_tree(args.rev, rep)
    return rep.emit()


if __name__ == "__main__":
    sys.exit(main())
