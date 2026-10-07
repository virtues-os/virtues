# The assistant driving a WebKit tab: spike

**Written 2026-10-07.** Whether the assistant can drive a native WKWebView
tab on the Mac well enough to build on. This was the riskiest assumption in
the browser design. [browser-spike.md](browser-spike.md) moved the browser
the owner sees off the box and onto the device, and WKWebView has no Chrome
DevTools Protocol. The harness was a ~300-line Swift app: one WKWebView plus a
localhost control port exposing the calls the assistant's tools would make
(navigate, snapshot, click by ref, type, press, scroll, screenshot, cookies).
It lived in a scratch directory. Tauri's Mac webview is this same WKWebView,
and wry hands it to native code, so the findings carry over.

## What answered yes

**Playwright's accessibility snapshot runs in WebKit unchanged.**
- The setup: Playwright 1.63's injected script (Apache-2.0), run through
  `callAsyncJavaScript` in an isolated `WKContentWorld`. The page cannot see or
  tamper with it.
- `ariaSnapshot(document.body, { mode: 'ai' })` returns the same
  refs-per-element outline Playwright MCP gives models, in 7–80 ms.
- Refs resolve back to elements through the script's own
  `_lastAriaSnapshotForQuery`.

**Native input works, and synthetic input does not.** The same page logged
every event's `isTrusted`:

| | Synthetic DOM events (Playwright's `webViewInput`) | Real AppKit events (`NSEvent`) |
|---|---|---|
| What the page sees | every event `isTrusted=false` | every event `isTrusted=true` |
| Button, React `onClick` | works | works |
| Type into an input | **fails**: a synthetic mousedown never moves focus, so keys land on `<body>` | works |
| Enter submits a form | **fails** | works (`submit` trusted) |
| contenteditable | **fails** | works |
| React controlled input | **fails** | works |

`NSResponder.insertText(_:)` on the webview inserts a whole string as one
trusted `beforeinput`/`input` pair, and React's state follows. It is the fast
path for long text.

**It works with the window in the background, without taking focus.**
- `window.sendEvent` delivers nothing unless the window is key.
- Calling the webview's own responder methods directly does work with another
  app frontmost (`mouseDown(with:)`, `mouseUp(with:)`, `keyDown(with:)`,
  `keyUp(with:)`, `scrollWheel(with:)`). Every site test below passed that way,
  and the owner's browser stayed frontmost throughout.

**Real sites, all driven natively from the background:**

| Site | Task | Result |
|---|---|---|
| Hacker News | click a story's comments link by ref | pass |
| Wikipedia | click search, type, Enter | pass |
| Wikipedia | wheel scroll (900 px) | pass |
| react.dev | open the DocSearch modal (a React portal), type into its controlled input, pick a result | pass |
| Lexical playground | click into the rich-text editor and type | pass |
| X login page | snapshot reads cleanly | pass |
| X, logged in | read the bookmarks list | pass |
| X, logged in | wheel-scroll the bookmarks list | **fail**: see below |
| X, logged in | type a query into X's React search box and press Enter | pass |

Nothing was posted or liked.

**The login hands its session to native code.**
- The owner logged in to X with a password inside the spike's webview.
- `WKHTTPCookieStore` then held `auth_token` (HttpOnly, 40 chars), `ct0` (160)
  and `cf_clearance`, all readable natively.
- Those are the two values `x_bookmarks_sync` needs. The device → box handoff
  is a POST to the existing connect route.

**Sign in with Apple is native.** WebKit intercepts
`appleid.apple.com/auth/authorize` and shows macOS's own Sign in with Apple
sheet with Touch ID. No popup tab appears, and `createWebViewWith` is never
called. Ordinary `window.open` and `target=_blank` do go through
`createWebViewWith`, and the harness followed them into a new tab. In this run
the owner's Apple ID was not linked to their X account, so X offered sign-up.
They used their password instead.

**Speed.**
- `takeSnapshot` returns a 2560×1720 Retina image in 55–104 ms, about 1.5 MB as
  PNG. Downscale it, or use JPEG, before it reaches a model.
- Snapshots are 7–80 ms.
- Page loads matched Safari.

## What needs design

- **Hidden pages stop animation frames.** A fully covered window reports
  `document.hidden` and `requestAnimationFrame` never fires, so assistant code
  that waits on a frame hangs. That happened here. The fixes:
  - `WKPreferences.inactiveSchedulingPolicy = .none` (macOS 14+);
  - wait on timers, never on `requestAnimationFrame`.
- **Snapshots of content pages are large:**

  | Page | Lines | Chars | Approx. tokens |
  |---|---|---|---|
  | X login | 47 | 2,000 | 500 |
  | X bookmarks screen | 451 | 28,500 | 7,000 |
  | react.dev home | 590 | 38,000 | 9,500 |
  | Hacker News | 1,014 | 44,000 | 11,000 |
  | Wikipedia article | ~1,100 | 58,000–60,000 | 15,000 |

  The tools need a scoped snapshot (one region or a depth limit) and a diff
  against the last snapshot. Every outline would otherwise cost a page of
  context.
- **Refs go stale on re-rendering pages.** X renumbers refs on every snapshot.
  Once, a click on a ref taken moments earlier landed on X's back arrow rather
  than the intended button. That did not reproduce. The click tool must:
  - snapshot immediately before acting;
  - check that the point it will click actually hits the element;
  - check the element's name still matches.
- **Cross-origin iframes are outside the snapshot.** Google's sign-in button on
  X sits in an iframe and appears only as `iframe [ref=…]`.
  `callAsyncJavaScript` can target a frame, but it needs a `WKFrameInfo`, which
  WebKit hands out only through navigation and message callbacks.
- **Synthetic input is the wrong default on Mac.** Keep Playwright's
  `webViewInput` only where native events are unavailable: iOS, which this
  spike did not test.

- **Infinite scroll did not load more.** Six wheel events moved X's bookmarks
  page 6,500 px to its bottom, and no further posts loaded. The page stopped
  at six posts with no spinner. The same session's cookies then synced **89**
  bookmarks through `x_bookmarks_sync` on a dev box. So X's next-page loader
  never fired under background wheel input, and that was misread as the end of
  the list. Likely suspects: an `IntersectionObserver` or scroll handler that
  ignores a background window, or wheel events that scroll without the scroll
  events X listens for. Untested either way.

## The handoff, end to end

The spike app read `auth_token`/`ct0` from its cookie jar and POSTed them to a
dev core's `/api/connect/x/complete` (201, 84 ms). The values went app → box
and never reached a terminal. Reconciling created the per-credential X
Bookmarks applet, and one manual run wrote 89 rows to `data_content_bookmark`.
Logging in inside the app replaces the developer-tools paste for X.

## Not tested

- iOS (no public way to synthesize touches).
- Windows WebView2 (it has CDP).
- `<select>`, which opens a native menu in WKWebView.
- Drag and drop, file upload, double-click, modifier shortcuts and IME.
- `alert`/`confirm` dialogs, which need `WKUIDelegate` answers.
- Downloads.
- Bot detection that profiles input timing.
- The port into Tauri itself. wry's `with_webview` gives the `WKWebView`; from
  there it is objc2 calls for the content world, `takeSnapshot`, the
  `NSEvent`s and the cookie store.

## What it settles

The assistant can drive a WKWebView tab: trusted input, from the background,
without stealing focus, using Playwright's snapshot. Copy in the snapshot
script, not the 320 KB injected bundle: `ariaSnapshot.ts`, `roleUtils.ts`,
`domUtils.ts` and their isomorphic dependencies, with Playwright's NOTICE.
Drive the Mac with native events. A Chrome extension is not needed as a
fallback for the Mac.
