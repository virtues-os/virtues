/**
 * Backend origin for API + WebSocket calls.
 *
 * Two deployment shapes share this frontend:
 *  - **Desktop (box-served):** the box serves the app, so `/api` and `/ws` are
 *    same-origin and `backendOrigin` stays empty — nothing changes.
 *  - **Mobile (bundled SPA):** the app is bundled inside the Tauri binary at its
 *    own `tauri://` origin and reaches the box over the in-process iroh loopback.
 *    The mobile shell injects `window.__VIRTUES_BACKEND_ORIGIN__ =
 *    'http://127.0.0.1:7117'`, and we route `/api` + `/ws` there.
 *
 * A single global fetch interceptor (installFetchProxy) rewrites the app's
 * `/api` calls, so the ~110 existing `fetch('/api/...')` sites need no edits.
 * Only `/api` is rewritten — SvelteKit's own bundled assets/data (`/_app`,
 * route data) must keep loading from the local origin.
 */

let backendOrigin = '';

export function setBackendOrigin(origin: string): void {
  backendOrigin = origin.replace(/\/+$/, '');
}

export function getBackendOrigin(): string {
  return backendOrigin;
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
 * When a backend origin is set (mobile), install a global fetch shim routing
 * root-relative `/api` requests to it. No-op when unset (desktop, same-origin).
 */
export function installFetchProxy(): void {
  if (!backendOrigin || typeof window === 'undefined') return;
  const origin = backendOrigin;
  const orig = window.fetch.bind(window);

  const route = (p: string) => p.startsWith('/api');

  window.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
    if (typeof input === 'string' && route(input)) {
      return orig(origin + input, init);
    }
    if (input instanceof URL && input.origin === location.origin && route(input.pathname)) {
      return orig(origin + input.pathname + input.search, init);
    }
    if (input instanceof Request && input.url.startsWith(location.origin) && route(new URL(input.url).pathname)) {
      return orig(new Request(origin + input.url.slice(location.origin.length), input), init);
    }
    return orig(input as RequestInfo, init);
  }) as typeof window.fetch;
}

/**
 * Read the mobile shell's injected origin and wire routing. No-op on desktop
 * (the global is absent). Call once at client startup.
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
