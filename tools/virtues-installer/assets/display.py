#!/usr/bin/env python3
"""Virtues display kiosk — fullscreen WebKit onto the box's own /display route.

NEVER A BLANK SCREEN. The first golden image handed to another person (2026-08-19)
crash-looped its server behind a silently-retrying kiosk for 40 minutes; every
fact needed to diagnose it was knowable on the box and none of it was displayed —
the cost was a UART cable and a multi-hour investigation to find a missing
784-byte file. So: retry silently through a short grace window (a normal boot's
server takes seconds), then render a locally-generated diagnostic page — built
here, served over file://, depending on nothing that might be the broken thing —
and keep probing so a slow-but-healthy boot still lands on the real UI unattended.
"""
import glob
import html
import os
import subprocess
import time
import urllib.request

import gi

gi.require_version("Gtk", "3.0")
# Gdk needs its own require_version even though Gtk pulls it in: without this
# the import resolves to Gdk 4.0 and dies with "version '3.0', but '4.0' is
# already loaded", which surfaces as cage failing to start a session — an error
# that reads like a seat/DRM problem and sends you looking in the wrong place.
gi.require_version("Gdk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gdk, GLib, Gtk, WebKit2  # noqa: E402

URL = os.environ.get("VIRTUES_DISPLAY_URL", "http://localhost:8000/display")

# See DISPLAY_SHIM's Rust-side doc comment: the panel's EDID lies about its
# PHYSICAL size, so DPI can never be derived — but the pixel MODE is real.
# Zoom = mode width / 585 renders the same 585x329 CSS canvas the /display
# page is designed against, on any panel. VIRTUES_DISPLAY_ZOOM (env file)
# overrides for a panel that needs hand-tuning.
DESIGN_WIDTH_PX = 585.0


def _mode_width():
    try:
        for status in glob.glob("/sys/class/drm/card*-*/status"):
            with open(status) as fh:
                if fh.read().strip() != "connected":
                    continue
            with open(os.path.join(os.path.dirname(status), "modes")) as fh:
                first = fh.readline().strip()  # first mode = preferred
            if "x" in first:
                return float(first.split("x")[0])
    except Exception:
        pass
    return 1920.0  # the validated 7" panel; wrong panels beat a dead shim


def _logical_width():
    # Prefer what the compositor ACTUALLY set over the sysfs guess: a scaler
    # panel can advertise modes it never runs, and the sysfs read assumes the
    # first line won. GDK reports the monitor wlroots configured, in logical
    # px — the same units set_zoom_level divides — so this stays correct even
    # if an output scale ever appears. Gdk is imported below; by ZOOM time the
    # display connection exists.
    try:
        display = Gdk.Display.get_default()
        monitor = display.get_monitor(0) if display else None
        if monitor:
            w = float(monitor.get_geometry().width)
            if w > 0:
                return w
    except Exception:
        pass
    return _mode_width()


# The override must degrade to the derived zoom on any garbage — a typo in
# virtues.env would otherwise crash the shim at import, and Restart=always
# turns that into a permanently black panel (NEVER A BLANK SCREEN).
try:
    _zoom_env = float(os.environ.get("VIRTUES_DISPLAY_ZOOM", "") or 0.0)
except ValueError:
    _zoom_env = 0.0
ZOOM = (
    _zoom_env
    if 0.1 <= _zoom_env <= 16.0
    else max(0.5, min(8.0, _logical_width() / DESIGN_WIDTH_PX))
)
DATA_DIR = os.environ.get("VIRTUES_DATA_DIR", "/var/lib/virtues")
DIAG = "/run/virtues-diag.html"
GRACE_S = 15  # silent retries before the diagnostic page appears
PROBE_S = 4  # probe + refresh cadence while the diagnostic page is up

window = Gtk.Window()
window.fullscreen()
window.set_decorated(False)

# NO CACHE. Not a tuning knob — the panel showed a THREE-DAY-OLD UI after an
# upgrade, on 2026-08-10, and survived both a service restart and a power cycle.
# The box serves /display with `last-modified` and no `cache-control`, so WebKit
# is free to cache the shell heuristically; it kept the stale shell, and that
# shell names content-hashed JS chunks, so the whole old page came back from
# disk while the box served the new one. Diagnosing it from a photo of the
# screen cost an hour.
#
# DOCUMENT_VIEWER is WebKit's "disable the cache completely" model. A kiosk
# loading one page from localhost has nothing to gain from a cache and
# everything to lose: an appliance whose screen can lie about its own version
# is worse than one that re-fetches 40KB over loopback on every boot.
context = WebKit2.WebContext.get_default()
context.set_cache_model(WebKit2.CacheModel.DOCUMENT_VIEWER)

view = WebKit2.WebView()
view.set_zoom_level(ZOOM)
# Match the page background so the gap before first paint is the panel's own
# black, not WebKit's default white — a white flash on a dark 7" screen in a
# dim room is the most visible thing the box will ever do.
view.set_background_color(Gdk.RGBA(0.043, 0.059, 0.078, 1.0))


# ── the diagnostic page ──────────────────────────────────────────────────────
# Facts only, degraded per-section: any probe that fails prints "unavailable"
# rather than taking the page with it — every one of these may be the thing
# that is broken. Secrets are reported by PRESENCE only, never value: the env
# file this page describes holds the encryption key in plaintext.


def _out(cmd, timeout=3):
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return r.stdout.strip()
    except Exception:
        return ""


def _ok(cmd, timeout=3):
    try:
        return subprocess.run(cmd, capture_output=True, timeout=timeout).returncode == 0
    except Exception:
        return False


def _env_facts():
    env_file = os.path.join(DATA_DIR, "virtues.env")
    f = {"env_exists": False, "has_db_url": False, "has_key": False}
    try:
        with open(env_file, "r") as fh:
            body = fh.read()
        f["env_exists"] = True
        f["has_db_url"] = "\nDATABASE_URL=" in "\n" + body
        f["has_key"] = "\nVIRTUES_ENCRYPTION_KEY=" in "\n" + body
    except Exception:
        pass
    return f


def _pg_units():
    """The real cluster instances, named from /etc/postgresql. Asking the
    postgresql.service umbrella is how a bench unit showed "postgresql active"
    beside a dead cluster and a server that could never reach it (2026-08-25):
    the umbrella is a oneshot wrapper and stays active over a failed instance.
    The umbrella is the fallback only when no cluster config exists to name."""
    units = []
    try:
        for ver in sorted(os.listdir("/etc/postgresql")):
            vdir = os.path.join("/etc/postgresql", ver)
            for name in sorted(os.listdir(vdir)):
                if os.path.exists(os.path.join(vdir, name, "postgresql.conf")):
                    units.append("postgresql@" + ver + "-" + name)
    except Exception:
        pass
    return units or ["postgresql"]


def _facts():
    f = _env_facts()
    f["mounted"] = _ok(["mountpoint", "-q", DATA_DIR])
    f["fstab_disk"] = False
    try:
        with open("/etc/fstab") as fh:
            f["fstab_disk"] = "LABEL=virtues-data" in fh.read()
    except Exception:
        pass
    f["claimed"] = os.path.exists(os.path.join(DATA_DIR, ".claim-complete"))
    f["marker"] = os.path.exists(os.path.join(DATA_DIR, ".needs-firstboot"))
    f["units"] = []
    for unit in ["virtues"] + _pg_units() + ["virtues-firstboot", "virtues-qnnd"]:
        state = _out(["systemctl", "is-active", unit + ".service"]) or "unknown"
        f["units"].append((unit, state))
    f["virtues_state"] = dict(f["units"]).get("virtues", "unknown")
    f["hostname"] = _out(["hostname"]) or "unknown"
    f["version"] = _out(["/usr/local/bin/virtues", "--version"]) or "unavailable"
    f["uptime"] = _out(["uptime", "-p"]) or "unavailable"
    f["ip"] = _out(["ip", "-brief", "-4", "addr"]) or "unavailable"
    f["ntp"] = _out(["timedatectl", "show", "-p", "NTPSynchronized", "--value"])
    f["clock"] = time.strftime("%Y-%m-%d %H:%M UTC", time.gmtime())
    f["journal"] = _out(
        ["journalctl", "-u", "virtues.service", "-n", "10", "--no-pager", "-o", "cat"],
        timeout=5,
    ) or "journal unavailable"
    return f


def _verdict(f):
    """One line, plain language, worst confirmed fact first."""
    # FIRST BOOT IS NOT A FAULT. firstboot is "the longest boot there is" — it
    # claims the NVMe and builds the cluster over minutes — and while it runs the
    # disk is legitimately unmounted or seed-incomplete. Reporting "Storage
    # disconnected" / "Not provisioned" then, to a new owner in their first two
    # minutes, is a lie the page tells about a healthy box. So if the firstboot
    # unit is still activating, say so and stop; the storage/provisioning
    # verdicts below are only meaningful once it has finished.
    firstboot = dict(f["units"]).get("virtues-firstboot", "")
    if firstboot == "activating":
        # "Don't unplug" rides ONLY this verdict: this is the one window where
        # a yank interrupts the claim mid-write (2026-08-25: one left the
        # cluster's start.conf zero-length). The setup screen that replaces
        # this page is the all-clear — firstboot syncs before it can appear.
        return "Setting up — claiming the storage disk and preparing the database. This takes a few minutes. Don't unplug me."
    if f["fstab_disk"] and not f["mounted"]:
        return "Storage disconnected — the data disk is not mounted."
    if f["mounted"] and not f["env_exists"]:
        return "Not provisioned — virtues.env is missing from the data disk. First-boot seeding did not complete."
    if f["mounted"] and not f["has_db_url"]:
        return "Not provisioned — virtues.env has no DATABASE_URL. The server cannot start."
    if f["virtues_state"] not in ("active", "activating"):
        return "The Virtues server is not running (" + f["virtues_state"] + "). Its last words are below."
    return "Starting up — the server is not answering yet. This page will step aside when it does."


def _build_page():
    try:
        f = _facts()
        verdict = _verdict(f)
    except Exception as e:  # the page must render even if fact-gathering dies
        f, verdict = None, "Diagnostics failed to gather: " + str(e)
    esc = html.escape
    yes, no = "yes", "MISSING"
    rows = []
    if f:
        rows.append(("box", f["hostname"] + " · " + f["version"]))
        rows.append(("up", f["uptime"]))
        clock = f["clock"] + ("" if f["ntp"] == "yes" else " (unsynced — may be wrong)")
        rows.append(("clock", clock))
        rows.append(("net", f["ip"]))
        rows.append(("data disk", ("mounted" if f["mounted"] else "NOT MOUNTED") + (" · claim complete" if f["claimed"] else " · claim incomplete")))
        rows.append(("env file", (yes if f["env_exists"] else no) + " · DATABASE_URL " + (yes if f["has_db_url"] else no) + " · key " + ("present" if f["has_key"] else "absent")))
        units = " · ".join(u + " " + s for u, s in f["units"])
        rows.append(("units", units))
    # The units row wraps instead of truncating: it now names the real cluster
    # instances, and an ellipsis over the one row that explains a wedge is the
    # umbrella lie all over again in CSS.
    body_rows = "".join(
        "<div class='r" + (" wrap" if k == "units" else "") + "'><span class='k'>"
        + esc(k) + "</span><span class='v'>" + esc(v) + "</span></div>"
        for k, v in rows
    )
    journal = esc(f["journal"]) if f else ""
    page = (
        "<!doctype html><meta charset='utf-8'>"
        "<style>"
        "html{background:#0b0f14;color:#c8d0da;font:10px/1.5 monospace;margin:0}"
        "body{margin:8px}"
        ".lockup{color:#55636f;font-size:9px;letter-spacing:0.04em;margin:0 0 6px}"
        ".lockup .mk{font-size:11px;color:#46545f}"
        ".verdict{color:#e8b04b;font-size:12px;margin:0 0 8px}"
        ".r{display:flex;gap:6px;white-space:nowrap;overflow:hidden}"
        ".r.wrap{white-space:normal}"
        ".r.wrap .v{overflow:visible;text-overflow:clip}"
        ".k{color:#5c6773;min-width:58px;flex:none}"
        ".v{overflow:hidden;text-overflow:ellipsis}"
        "pre{color:#7a8494;margin:8px 0 0;font-size:8px;line-height:1.4;white-space:pre-wrap;word-break:break-all}"
        "</style>"
        "<div class='lockup'><span class='mk'>&there4;</span> Virtues</div>"
        "<p class='verdict'>" + esc(verdict) + "</p>"
        + body_rows
        + "<pre>" + journal + "</pre>"
    )
    tmp = DIAG + ".tmp"
    with open(tmp, "w") as fh:
        fh.write(page)
    os.replace(tmp, DIAG)


# ── loading + escalation ─────────────────────────────────────────────────────

state = {"first_fail": None, "diag": False}


def _probe():
    """Is the real UI ready? Asked out-of-band so the answer never depends on
    what the WebView happens to be showing."""
    try:
        with urllib.request.urlopen(URL, timeout=2) as r:
            return 200 <= r.status < 400
    except Exception:
        return False


def _tick():
    if not state["diag"]:
        return False
    if _probe():
        state["diag"] = False
        state["first_fail"] = None
        view.load_uri(URL)
        return False
    _build_page()
    view.reload()
    return True


def _failed():
    """One load of the real UI failed, network-level or HTTP. Retry silently
    inside the grace window (a normal boot's server takes seconds and an owner
    should never see machinery), then escalate to the diagnostic page — never
    to nothing. The old version of this retried silently FOREVER, which is how
    a hollow box stayed undiagnosable without a serial cable (2026-08-19)."""
    if state["diag"]:
        return
    now = time.monotonic()
    if state["first_fail"] is None:
        state["first_fail"] = now
    if now - state["first_fail"] >= GRACE_S:
        state["diag"] = True
        try:
            _build_page()
            view.load_uri("file://" + DIAG)
        except Exception:
            state["diag"] = False  # keep retrying the real UI rather than dying
        GLib.timeout_add_seconds(PROBE_S, _tick)
    else:
        GLib.timeout_add_seconds(3, lambda: (state["diag"] or view.load_uri(URL), False)[1])


def _retry(view_, event, uri, error):
    """load-failed for the real UI escalates; for the diagnostic page itself it
    must never loop — fall through to WebKit's own error page, which at least
    says something."""
    if uri and uri.startswith("file://"):
        return False
    _failed()
    return True  # we handled it; suppress WebKit's own error page


def _check_http(view_, event):
    """`load-failed` never fires for an HTTP error — a 502/500 from the box
    mid-start is a *successful* load of the wrong document. Check the status
    once the load settles; a good load of the real UI resets the grace clock."""
    if event != WebKit2.LoadEvent.FINISHED:
        return
    uri = view_.get_uri() or ""
    if uri.startswith("file://"):
        return
    res = view_.get_main_resource()
    resp = res.get_response() if res else None
    if resp and resp.get_status_code() >= 400:
        _failed()
    elif resp:
        state["first_fail"] = None


view.connect("load-failed", _retry)
view.connect("load-changed", _check_http)
view.load_uri(URL)

window.add(view)
window.connect("destroy", Gtk.main_quit)
window.show_all()
Gtk.main()
