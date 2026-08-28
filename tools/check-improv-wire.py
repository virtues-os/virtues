#!/usr/bin/env python3
"""Fail when the Swift Improv client and the box's parser disagree on a packet.

The box parses `0x83 PairConsume` as exactly four length-prefixed strings and
REJECTS a fifth (`if !rest.is_empty() { return Err(InvalidPacket) }`). The iOS
client builds that packet by hand, in another language, in another file. Nothing
compiles the two together.

So they drifted. 0x83 dropped its leading 6-digit code on 2026-08-24; the Swift
kept pushing it, sent five strings forever, and every BLE pair failed at
`parse_rpc` before reaching a handler. It went unnoticed because the two halves
look correct in isolation and the only integration is a radio.

This is a RATCHET on arity, not a protocol test: it asks whether the two sides
agree on HOW MANY fields travel, which is the specific thing that broke and the
one a reviewer cannot see from either file alone. Field ORDER and meaning are
still on the author — but a reordering keeps the packet parseable and shows up
as wrong behaviour, whereas an arity change makes the box reject every packet
with no message the user can act on.

  Usage:  tools/check-improv-wire.py
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RUST = ROOT / "crates/virtues-improv/src/protocol.rs"
SWIFT = ROOT / "apps/web/plugins/reach/ios/Sources/ImprovClient.swift"


def box_arity() -> int:
    """How many strings `parse_rpc` takes for 0x83, from the branch itself."""
    src = RUST.read_text()
    m = re.search(r"^\s*0x83 => \{(.*?)^\s*\}", src, re.S | re.M)
    if not m:
        sys.exit(f"error: no 0x83 branch found in {RUST.relative_to(ROOT)}")
    branch = m.group(1)
    if "rest.is_empty()" not in branch:
        sys.exit(
            "error: the 0x83 branch no longer rejects trailing data, so arity is "
            "not enforced on the wire and this check cannot mean anything"
        )
    return len(re.findall(r"take_string\(", branch))


def client_arity() -> int:
    """How many strings the Swift client pushes into the 0x83 payload."""
    src = SWIFT.read_text()
    # `func pair(` through the write that sends it — the payload is built inline.
    m = re.search(r"func pair\(.*?buildRPC\(command: 0x83", src, re.S)
    if not m:
        sys.exit(f"error: no 0x83 packet builder found in {SWIFT.relative_to(ROOT)}")
    body = m.group(0)
    # Definitions of the helper are not calls to it.
    return len(re.findall(r"^\s*pushString\(", body, re.M))


def main() -> int:
    box, client = box_arity(), client_arity()
    if box != client:
        print(
            f"Improv 0x83 wire mismatch: the box parses {box} strings, the iOS "
            f"client sends {client}.\n\n"
            f"  box:    {RUST.relative_to(ROOT)} (parse_rpc, 0x83)\n"
            f"  client: {SWIFT.relative_to(ROOT)} (func pair)\n\n"
            "The box rejects a packet with trailing data, so a mismatch is not a "
            "degraded pair — it is no pair at all, and the failure reaches the "
            "user as a timeout with nothing to act on.",
            file=sys.stderr,
        )
        return 1
    print(f"✓ Improv 0x83 agrees: {box} strings on both sides")
    return 0


if __name__ == "__main__":
    sys.exit(main())
