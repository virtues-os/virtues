# The Browser

The owner's browser, built into the Mac app, used by both the owner and the
assistant. Shipped in Mac 1.0.30 and box v0.1.10-staging.96/.97 (2026-10-07 to
10-10). The two spikes it rests on are [browser-spike.md](browser-spike.md),
which rejected a box-side browser streamed to the owner, and
[webkit-agent-spike.md](webkit-agent-spike.md), which showed that WebKit can be
driven without a DevTools protocol.

## Shape

- **One browser, in the Mac app.** It is a native `WKWebView` that the shell
  places over the UI's Browser tab (`apps/web/src-tauri/src/browser_host.rs`).
  The tab opens beside the current view. The view has its own persistent
  cookie jar, separate from the app's UI and from Safari.
- **The box drives it over a websocket.** While the app runs, the shell holds
  `/ws/browser` open to the box (`virtues-core/src/browser.rs`). Each
  `browser_*` tool call becomes one request on that socket. The shell runs the
  requests in arrival order, one at a time. The tools are offered only while a
  browser is connected, and they run without a permission prompt.
- **Reading** uses Playwright's injected script (vendored, Apache-2.0) in an
  isolated content world, which returns an accessibility outline with element
  refs. **Acting** sends AppKit events straight to the view's responder
  methods. They arrive trusted and work with the window behind other apps.
- **Source logins** ("Log in to X", "Log in to Instagram") open in the same
  pane. The shell waits for the cookies that mean "logged in", then hands the
  site's cookie jar to the box's ordinary connect route.

## What the owner sees

- While the assistant acts:
  - a pill reading "Your assistant is using the browser", with Take control and
    Stop;
  - an accent frame around the page card;
  - a cursor and pulse at each click, hidden from the screenshots the model
    sees.
- **Clicking or typing in the page while the assistant drives takes control.**
  An `NSEvent` local monitor sees only real input, because the assistant's
  events go straight to the view and never pass through the app's event queue.
  The assistant's next step is refused until the owner presses Hand back.
- **Stop** ends the chat reply that is using the browser. A reply running on
  another device can't be stopped from the Mac, so Stop takes control instead.
- **`browser_handoff`** hands the browser to the owner for a step only they can
  do, with the reason, Done and I can't. It waits up to 15 minutes. There is no
  approve/deny card anywhere, which was the owner's call.
- **The shell refuses to type into password, one-time-code and card fields**,
  and tells the model to hand off instead.
- **Activity** is a row of the steps taken, each with a thumbnail. It is off
  unless the owner opens it. The owner's judgment was that people watch the
  page or leave it to the assistant, so a log is for when something needs
  explaining.

## Traps found while building it

- **Tauri's `cookies_for_url` compares domains exactly.** Instagram sets
  `sessionid` for `.instagram.com`, but its login page is on
  `www.instagram.com`. The login therefore waited forever after a successful
  sign-in. X worked only because its login page is on `x.com` itself. Both
  login paths now read the whole cookie store and filter it the way a browser
  would (`browser.rs` `jar_for`, with tests). A real Instagram login through
  the pane connected, and the first sync completed.
- **The UI and the native view measure from different tops.** The UI measures
  from below the title bar, and the native view is placed from the top of the
  window's content, which runs under it. Unadjusted, the page drew one title
  bar too high and covered the address bar. `set_bounds` adds the title-bar
  height, which is zero in full screen.
- **Clicks land one title bar too high** for the same reason. `mouse()`
  corrects using the measured `bounds.height - innerHeight`.
- **A pane created at 1x1 makes every click miss.** It is created hidden at
  1100x800 instead.
- **X's timeline loads nothing more for a page WebKit thinks is hidden.**
  Occlusion detection is turned off through WebKit SPI
  (`_setWindowOcclusionDetectionEnabled:`), and the inactive scheduling
  policy is `none`.
- **WebKit drops an unretained content world, and every global in it.** The
  agent world is held in a `thread_local` for the life of the app.
- **A takeover that only Hand back ends outlives its task.** A click in the
  page just after a reply finished counted as taking control, and every
  later chat was refused. A takeover now lapses after 20 quiet seconds
  (`TAKEOVER_LAPSES`), and keeps being refused while the assistant keeps
  trying.
- **CSS transitions freeze part way in a hidden pane**, so the click cursor
  jumps instead of gliding.
- **A model that calls open and snapshot in the same step means "open, then
  read".** Operations therefore run in arrival order, never in parallel.
- **Mac builds made on a laptop are stamped `dev`**, so the app keeps serving
  the box's older UI bundle over its own. To see local UI changes, build with
  `GIT_DESCRIBE` set above the box's version.

## Open

- Cross-origin iframes are not in the snapshot.
- The shell crashed once (SIGABRT, 2026-10-07 16:30), in a build whose crash
  report had no symbols. It has not been seen since.
- Browsing unattended on the box, using sessions handed over from the Mac, so
  syncs keep running while the Mac sleeps. This is the next large step.
- There is no browser on the iPhone.
