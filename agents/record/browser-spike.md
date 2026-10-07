# Browser on the box: spike

**Written 2026-10-07.** One afternoon on the spare Dragon (`dragon2`) testing
whether the box itself can run the browser that three features want: the
assistant driving a browser, the owner seeing and using it as a tab, and the
owner logging in to X or Instagram so the bookmark syncs get a session without
anyone copying cookies out of developer tools. Nothing here is built into the
product. The scripts lived in a scratch directory and are described below so
the numbers can be reproduced.

## The question

Where does the browser run? On the owner's device (a webview in the Tauri app)
or on the box (a headless Chromium the core drives over the DevTools protocol,
streamed to the app)? The agent loop, the tools and the applets all run on the
box, so a box browser serves all three without a hop to a device that may be
asleep. The spike tested whether the box can carry it.

## Setup

- Chrome for Testing 153.0.8010.12 for **linux-arm64**, from
  `cdn.playwright.dev/builds/cft/<ver>/linux-arm64/chrome-linux-arm64.zip`
  (196 MB zip, 393 MB unpacked). Google started publishing linux-arm64 builds
  in August 2026; Playwright 1.63 pulls them. The installer avoided Chromium
  until now because Ubuntu 24.04's `chromium-browser` is a snap stub
  (`tools/virtues-installer/src/install.rs`). A pinned zip is not a snap.
- `ldd` found every shared library already on the box image. No apt packages
  were needed.
- Run from `/dev/shm`, because `radxa` cannot write the data partition and root
  had 753 MB free. Launched `--headless=new` with a fixed
  `--remote-debugging-port` on loopback, reached from a Mac through an SSH
  tunnel, and driven by a ~60-line flat-session CDP client on Node's built-in
  WebSocket.

## What was measured

**Rendering is software.** `SystemInfo.getInfo` reports ANGLE on SwiftShader
(Vulkan), WebGL `unavailable_software`, and no hardware GPU. The Adreno driver
is never touched.

**Memory** (the unit's cgroup `MemoryCurrent`, 2 s samples):

| State | Memory |
|---|---|
| Browser, blank tab | 190–210 MB |
| X login flow loaded, peak | ~400 MB |
| X settled after 30 s | ~335–365 MB |
| Instagram + Hacker News + blank tab (sum of PSS, earlier run) | ~615 MB |

The box had 9.3–9.7 GB available throughout the capped run, and the load
average stayed under 1.7.

**Speed on the box:**

| Page | Load | Snapshot | Screenshot |
|---|---|---|---|
| x.com login, cold profile | 17.9 s | 90 ms | 245 ms |
| Instagram login | 1.4 s | 104 ms | 200 ms |
| Hacker News | 0.6 s | 346 ms | 229 ms |

**The agent's view of a page is small.** A Playwright-MCP-style snapshot (one
line per meaningful accessibility node, each with a ref) came to:

| Page | Snapshot |
|---|---|
| X login | 8 lines |
| Instagram login | 77 lines, ~610 tokens |
| Hacker News | 199 lines, ~1,800 tokens |

X's login reads cleanly as a dialog with the buttons Continue with phone,
Continue with Google and Continue with Apple, plus an "Email or username"
textbox.

**Bot gating:**
- X's Cloudflare front answers the default `HeadlessChrome/153` user agent
  with a 403 page ("Access to x.com was denied").
- With a normal Chrome user agent set at launch, the login flow renders.
- `navigator.webdriver` is `false`: headless=new, fixed port, and no
  `--enable-automation`.
- Instagram serves its login page even to the headless user agent.
- Whether either site challenges an actual login from this browser is **not
  yet tested**. It needs the owner to type their own password.

**Live view and takeover work over plain CDP.**
- `Page.startScreencast` sends JPEG frames at quality 60, 21–70 KB each.
  Frames come only when the page changes, about 2–3 per second on a static page.
- Mouse, wheel, text and key events go back through `Input.dispatch*` and land
  in 16–23 ms for a click plus ten characters over loopback.
- Enter submits a form only when its `keyDown` carries `text: "\r"`.
- In the spike, frames went over server-sent events and input went over POST
  to a local page.

**Cookies are readable from outside the page.** `Network.getCookies` returns
the HttpOnly ones that X and Instagram set (`__cf_bm`, `ig_did`, `datr`, and
so on). That is how a login on the box would hand `auth_token`/`ct0` or the
Instagram jar to the sync applets without the values ever leaving the box.

## What broke: the first run hung the box

The first run had no memory cap. Its X tab stopped answering CDP, and within
about a minute `dragon2` stopped answering ssh, ping and Tailscale. The
hardware watchdog (`10-virtues-watchdog.conf`, `RuntimeWatchdogSec=30s`) reset
it, and it was back about five minutes later. The user journal stops at
15:59:11 with no OOM line. `radxa` cannot read the kernel journal, so the cause
is **not proven**. The first run differed from the clean second run in three
ways:

1. No cgroup cap. The second run was a `systemd-run --user` unit with
   `MemoryMax=1500M`, `MemorySwapMax=0` and `CPUQuota=300%`. Without a cap, a
   runaway renderer can push the box into zram thrash, a livelock that starves
   PID 1's watchdog pings and logs no OOM.
2. `--disable-dev-shm-usage`, which puts Chrome's shared memory in `/tmp` on
   the 90%-full root filesystem.
3. The user agent was swapped per session (`Network.setUserAgentOverride`)
   after Cloudflare had already 403'd the same profile, rather than set at
   launch.

Whatever the cause, the shipped shape must have all three fixes:
- A capped, swapless unit, so a bad page kills a renderer, not the box.
- Shared memory left in `/dev/shm`, where the cgroup is charged for it.
- The user agent set at launch.

## The live view failed the owner's eye test

Later the same day the owner opened the live view to log in to X. It ran
`dragon2` (on office Wi-Fi) → Tailscale's DERP relay in Dallas → a Mac. The
verdict: about 2 fps, blurry, unusable. Nothing about the box can fix it:

- The source renders at 1× in software.
- Frames are JPEG at quality 60.
- CDP screencast tops out at a few frames per second. Steel left it for
  WebRTC H.264 for exactly this reason.
- On a relayed path every frame and every click crosses the internet twice.

Clicks did land, but X's "Continue with Apple" and "Continue with Google" open
popup tabs that a single-tab stream never shows. A live view has to follow
`Target.targetCreated` popups or OAuth looks dead.

**Outcome: the browser the owner sees runs on the owner's device.** That is
where Claude's desktop browser, Atlas, Comet and Dia run it. The box engine
above remains the option for unattended browsing that nobody watches, such as
an applet. **Do not rebuild a streamed tab as the owner's browser.**

## What it suggests

These are directions for a plan, not decisions. The live-tab and source-login
items below are superseded by the verdict above; the box engine stands only
for unattended work.

- **One engine, on the box.** A pinned Chrome for Testing arm64 build, fetched
  and hash-checked by the installer, runs as a sidecar in the shape of the door
  (`virtues-core/src/door.rs`):
  - its own user;
  - `MemoryMax` with no swap;
  - a `StateDirectory` on the data partition for profiles;
  - loopback and private address ranges denied, so a page or a steered agent
    cannot reach the core API, Postgres or the LAN. This is the browser's
    version of `fetch/guard.rs`.
- **The core speaks CDP itself.** Either a thin client over tokio-tungstenite,
  which is what vercel-labs/agent-browser does, or `chromey`, the maintained
  chromiumoxide fork. Nothing in the repo speaks CDP today.
- **Three consumers of that engine:**
  1. Assistant tools: navigate, snapshot (refs), click(ref), type(ref), key,
     scroll and screenshot. Screenshots return as a `ToolAttachment`, the way
     `read_asset` returns images. Irreversible steps go through the existing
     approval card (`check_tool_permission` / `sudo_gate`). Browser tools need
     their own timeout entry beside `code_interpreter`'s.
  2. A live tab in the app: screencast frames box→app and input app→box over a
     websocket, modelled on `/ws/terminal`, which already passes through the
     iroh splice.
  3. Source login: a "Log in to X" button opens that live tab on a profile kept
     for that source. When the session cookies appear, the core reads them over
     CDP and stores them with `vault::finalize_apikey_credential` (then
     `reconcile_templates`, as the HTTP route does). The paste form stays as the
     fallback.
- **Keep logged-in profiles away from the assistant.** The assistant should
  browse in a profile without the owner's sessions. Our own agent loop gets the
  raw model's prompt-injection rate, not a vendor harness's.
- **A streamed browser is not a daily browser.** It is fine for logging in,
  watching the assistant and taking over from it. It has no password-manager
  autofill and it is a video of a page. If a native everyday browser is wanted,
  it is a device webview, and the box engine is still the one the assistant
  and the applets use.

## Not yet tested

- An actual X and Instagram login through the live view: challenges, 2FA, and
  whether `auth_token`/`ct0` appear.
- Feeding those cookies to `x_bookmarks_sync` and running it.
- Screencast frame rate and input latency over an iroh relay rather than
  loopback.
- Memory with several tabs and an assistant session open together.
- Google and Apple sign-in. Research says Google refuses automated and embedded
  browsers; this was not tried.
- The first-run hang's root cause, which needs the kernel journal (`sudo`).
