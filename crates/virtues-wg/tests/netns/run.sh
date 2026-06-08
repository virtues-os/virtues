#!/usr/bin/env bash
# Network-namespace harness for the remote-access auto-heal contract (Phase 6).
#
# It builds two Linux netns — `box` and `client` — joined by a veth pair, runs a
# real kernel WireGuard tunnel between them, then mutates the box's underlay
# endpoint to model the three failure modes that the blind-rendezvous design
# must survive:
#
#   1. prefix rotation  — ISP hands out a new IPv6 prefix
#   2. NAT/port change   — the box's reachable UDP port moves
#   3. ISP swap          — the box lands on a wholly different network
#
# For each, it asserts: tunnel up → mutation breaks it → applying the new
# endpoint (what the phone's RendezvousClient does after a fetch-on-failure)
# heals it. The rendezvous publish/fetch itself is exercised by Rust tests; here
# we validate the transport-level claim that re-pointing the peer endpoint is
# sufficient to recover a live tunnel.
#
# Linux + root + kernel `wireguard` module required. On anything else it SKIPs
# (exit 0) so cross-platform CI stays green; a Linux job runs it for real.
#
#   sudo crates/virtues-wg/tests/netns/run.sh
set -euo pipefail

OVL_BOX=10.99.0.1
OVL_CLI=10.99.0.2
PORT_A=51820
PORT_B=51821
PFX_A=2001:db8:a::
PFX_B=2001:db8:b::

pass=0
fail=0

log()  { printf '  %s\n' "$*"; }
ok()   { printf '  \033[32mPASS\033[0m %s\n' "$*"; pass=$((pass + 1)); }
bad()  { printf '  \033[31mFAIL\033[0m %s\n' "$*"; fail=$((fail + 1)); }
skip() { printf 'SKIP: %s\n' "$*"; exit 0; }

# ─── prerequisites (skip, don't fail, when unmet) ───────────────────────────
[ "$(uname -s)" = "Linux" ] || skip "Linux-only (network namespaces)"
[ "$(id -u)" -eq 0 ] || skip "needs root (run with sudo)"
for c in ip wg ping; do command -v "$c" >/dev/null || skip "missing '$c'"; done
# Confirm the kernel can actually create a wireguard interface.
if ! ip link add wgprobe type wireguard 2>/dev/null; then
    skip "kernel 'wireguard' type unavailable"
fi
ip link del wgprobe 2>/dev/null || true

# ─── lifecycle ──────────────────────────────────────────────────────────────
cleanup() {
    ip netns del box 2>/dev/null || true
    ip netns del client 2>/dev/null || true
}
trap cleanup EXIT
cleanup # start from a clean slate

keypair() { # -> "<priv> <pub>"
    local k
    k=$(wg genkey)
    printf '%s %s' "$k" "$(printf '%s' "$k" | wg pubkey)"
}

# box_addr <prefix> -> the box's underlay IPv6 on that prefix
box_addr() { printf '%s1' "$1"; }
cli_addr() { printf '%s2' "$1"; }

setup() {
    read -r BOX_PRIV BOX_PUB < <(keypair)
    read -r CLI_PRIV CLI_PUB < <(keypair)

    ip netns add box
    ip netns add client

    # Underlay link (the "WAN") between the two namespaces.
    ip link add veth-box netns box type veth peer name veth-cli netns client
    ip -n box link set lo up
    ip -n client link set lo up
    ip -n box link set veth-box up
    ip -n client link set veth-cli up
    ip -n box   addr add "$(box_addr "$PFX_A")/64" dev veth-box
    ip -n client addr add "$(cli_addr "$PFX_A")/64" dev veth-cli

    # WireGuard interface in each namespace.
    ip -n box   link add wg0 type wireguard
    ip -n client link add wg0 type wireguard
    ip netns exec box   wg set wg0 private-key <(printf '%s' "$BOX_PRIV") listen-port "$PORT_A"
    ip netns exec client wg set wg0 private-key <(printf '%s' "$CLI_PRIV")
    ip -n box   addr add "$OVL_BOX/24" dev wg0
    ip -n client addr add "$OVL_CLI/24" dev wg0
    ip -n box   link set wg0 up
    ip -n client link set wg0 up

    # Peers. The box accepts the client; the client dials the box's endpoint.
    ip netns exec box wg set wg0 peer "$CLI_PUB" allowed-ips "$OVL_CLI/32"
    point_client_at "$(box_addr "$PFX_A")" "$PORT_A"
}

# Re-point the client's peer endpoint — the transport-level action the phone
# performs after fetching the box's fresh endpoint from the rendezvous.
point_client_at() { # <box-underlay-v6> <port>
    ip netns exec client wg set wg0 \
        peer "$BOX_PUB" \
        endpoint "[$1]:$2" \
        persistent-keepalive 3 \
        allowed-ips "$OVL_BOX/32"
}

# tunnel_up -> 0 if the client can reach the box over the overlay
tunnel_up() {
    ip netns exec client ping -c1 -W2 "$OVL_BOX" >/dev/null 2>&1
}

# Poll for a desired tunnel state (handshakes/keepalives take a moment).
wait_for() { # <up|down> <seconds>
    local want="$1" deadline=$((SECONDS + $2))
    while [ "$SECONDS" -lt "$deadline" ]; do
        if [ "$want" = up ] && tunnel_up; then return 0; fi
        if [ "$want" = down ] && ! tunnel_up; then return 0; fi
        sleep 1
    done
    [ "$want" = up ] && tunnel_up
}

# scenario <name> <mutate-fn> — assert: up -> mutate breaks it -> re-point heals
scenario() {
    local name="$1" mutate="$2"
    log "scenario: $name"
    setup
    if wait_for up 8; then ok "$name: baseline tunnel up"; else bad "$name: never came up"; cleanup; return; fi

    "$mutate" # mutates the box endpoint WITHOUT telling the client

    if wait_for down 8; then ok "$name: tunnel broke after endpoint moved"; else bad "$name: tunnel did not break"; fi

    "${mutate}_recover" # phone re-resolves via rendezvous and re-points

    if wait_for up 12; then ok "$name: auto-healed after re-point"; else bad "$name: did not heal"; fi
    cleanup
}

# ── 1. prefix rotation: box's underlay v6 moves to a new prefix ──────────────
mut_prefix() {
    ip -n box addr del "$(box_addr "$PFX_A")/64" dev veth-box
    ip -n box addr add "$(box_addr "$PFX_B")/64" dev veth-box
    ip -n client addr add "$(cli_addr "$PFX_B")/64" dev veth-cli
}
mut_prefix_recover() { point_client_at "$(box_addr "$PFX_B")" "$PORT_A"; }

# ── 2. NAT/port change: the box's reachable UDP port moves ───────────────────
mut_port() { ip netns exec box wg set wg0 listen-port "$PORT_B"; }
mut_port_recover() { point_client_at "$(box_addr "$PFX_A")" "$PORT_B"; }

# ── 3. ISP swap: new prefix AND new port at once ─────────────────────────────
mut_swap() {
    ip -n box addr del "$(box_addr "$PFX_A")/64" dev veth-box
    ip -n box addr add "$(box_addr "$PFX_B")/64" dev veth-box
    ip -n client addr add "$(cli_addr "$PFX_B")/64" dev veth-cli
    ip netns exec box wg set wg0 listen-port "$PORT_B"
}
mut_swap_recover() { point_client_at "$(box_addr "$PFX_B")" "$PORT_B"; }

echo "netns WireGuard auto-heal harness"
scenario "prefix-rotation" mut_prefix
scenario "nat-port-change" mut_port
scenario "isp-swap"        mut_swap

echo
echo "netns harness: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
