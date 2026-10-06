/**
 * Backend origin for API + WebSocket calls.
 *
 * Two deployment shapes share this frontend:
 *  - **Box-served** (a browser on the LAN, the box's panel, and the Windows and
 *    Linux apps): the box serves the app, so `/api` and `/ws` are same-origin
 *    and `backendOrigin` stays empty — nothing changes.
 *  - **The app's own copy** (the phone, and the Mac since 2026-09-29): the app
 *    carries the SPA at its own `virtues://` origin and reaches the box over
 *    the in-process iroh loopback. The shell injects
 *    `window.__VIRTUES_BACKEND_ORIGIN__ = 'http://127.0.0.1:7117'`, and we
 *    route `/api` + `/ws` there.
 *
 * A single global fetch interceptor (installFetchProxy) rewrites the app's
 * box paths (BACKEND_PREFIXES), so the existing `fetch('/api/...')` sites need
 * no edits. Only box paths are rewritten: SvelteKit's own bundled assets and
 * data (`/_app`, route data) must keep loading from the local origin.
 */

let backendOrigin = '';

export function setBackendOrigin(origin: string): void {
  backendOrigin = origin.replace(/\/+$/, '');
}

export function getBackendOrigin(): string {
  return backendOrigin;
}

/**
 * The box's paths. The shell's `serve_ui` answers exactly these with a 404 on
 * the app's own origin (src-tauri), so the two lists move together.
 * `/auth/session` is the session gate: miss it and the app thinks it's
 * unpaired and bounces to the connect screen. `/health` matters because the
 * app's origin answers any other extension-less path with the SPA's HTML and a
 * 200, so the box-upgrade watcher would never see the box go down. `/face` is
 * an applet face's own files and query bridge.
 */
const BACKEND_PREFIXES = ['/api', '/auth', '/webhook', '/health', '/face', '/ws', '/oauth'];

function isBackendPath(pathname: string): boolean {
  return BACKEND_PREFIXES.some((pre) => pathname === pre || pathname.startsWith(pre + '/'));
}

/**
 * Resolve `raw` against this page and say whether it names this page's own
 * origin. Compared by protocol and host rather than `URL.origin`, which is
 * the opaque "null" for a non-special scheme like `virtues:`.
 */
function sameOrigin(raw: string): URL | null {
  if (typeof location === 'undefined') return null;
  let u: URL;
  try {
    u = new URL(raw, location.href);
  } catch {
    return null;
  }
  return u.protocol === location.protocol && u.host === location.host ? u : null;
}

/**
 * Absolute URL for a backend path that the browser resolves from MARKUP rather
 * than through `window.fetch` — `<iframe src>`, `<img src>`, `<video src>`,
 * CSS `url()`, an XHR. The fetch shim below cannot see these: it wraps
 * `window.fetch`, and an attribute-driven load never goes through it. In the
 * app's own copy they would otherwise resolve against the `virtues://` origin,
 * which serves no backend routes, and fail silently (an empty iframe, a broken
 * image).
 *
 * A root-relative path is the box's, and so is a box path on this page's own
 * origin spelled out in full. Any other URL (an Unsplash cover, a `data:` or `blob:`
 * one) loads as written. No-op where the box serves the app: `backendOrigin`
 * is empty and the path is already same-origin.
 */
export function backendUrl(path: string): string {
  if (!backendOrigin) return path;
  if (path.startsWith('/') && !path.startsWith('//')) return backendOrigin + path;
  const u = sameOrigin(path);
  return u && isBackendPath(u.pathname) ? backendOrigin + u.pathname + u.search + u.hash : path;
}

/** Base WebSocket URL (y-websocket appends room/pageId). */
export function getWsUrl(path = '/ws/yjs'): string {
  if (backendOrigin) {
    return backendOrigin.replace(/^http/, 'ws') + path;
  }
  const proto = typeof window !== 'undefined' && location.protocol === 'https:' ? 'wss:' : 'ws:';
  const host = typeof window !== 'undefined' ? location.host : 'localhost:8000';
  return `${proto}//${host}${path}`;
}

/**
 * When a backend origin is set (the app's own copy), install a global fetch
 * shim routing box paths to it. No-op when unset (the box serves the app).
 *
 * Every input is resolved against this page first, because a box path does
 * not always arrive root-relative: SvelteKit's load `fetch` passes an absolute
 * `virtues://localhost/auth/session` after the first navigation, and a URL or
 * Request carries the page's origin too. Missed, that request lands on the
 * app's own origin, 404s, and a load quietly takes its offline branch.
 */
export function installFetchProxy(): void {
  if (!backendOrigin || typeof window === 'undefined') return;
  const origin = backendOrigin;
  const orig = window.fetch.bind(window);

  window.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
    const raw = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    const u = sameOrigin(raw);
    if (u && isBackendPath(u.pathname)) {
      const target = origin + u.pathname + u.search;
      // A Request keeps its method, headers and body across the move.
      return orig(input instanceof Request ? new Request(target, input) : target, init);
    }
    return orig(input, init);
  }) as typeof window.fetch;
}

/**
 * Read the shell's injected origin and wire routing. No-op where the box
 * serves the app (the global is absent). Call once at client startup.
 */
export function initBackendFromShell(): void {
  const injected =
    typeof window !== 'undefined'
      ? (window as unknown as { __VIRTUES_BACKEND_ORIGIN__?: string }).__VIRTUES_BACKEND_ORIGIN__
      : undefined;
  if (typeof injected === 'string' && injected) {
    setBackendOrigin(injected);
    installFetchProxy();
  }
}
