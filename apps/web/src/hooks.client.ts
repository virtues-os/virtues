// Client-side SvelteKit hooks.
//
// The only thing here today is the CSRF wrapper for `window.fetch`. Every
// state-changing fetch (POST/PUT/PATCH/DELETE) to a same-origin or relative
// URL gets the current `virtues.csrf-token` cookie copied into an
// `X-CSRF-Token` header. The backend middleware refuses state-changing
// session-bearing requests without this header — completing the double-submit
// cookie pattern.
//
// No-op when:
//   - method is GET/HEAD/OPTIONS (server doesn't gate these)
//   - URL is cross-origin (the cookie scope wouldn't apply anyway)
//   - the caller already set `X-CSRF-Token` manually
//   - no CSRF cookie exists yet (first paint before pairing — server will
//     issue one on the next response, then it auto-applies on the one after)

const MUTATING_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);
const CSRF_COOKIE_NAMES = ["__Host-virtues.csrf-token", "virtues.csrf-token"];
const CSRF_HEADER = "X-CSRF-Token";

function readCsrfCookie(): string | null {
	if (typeof document === "undefined") return null;
	const entries = document.cookie.split(";").map((c) => c.trim());
	for (const name of CSRF_COOKIE_NAMES) {
		const prefix = `${name}=`;
		const hit = entries.find((e) => e.startsWith(prefix));
		if (hit) return decodeURIComponent(hit.slice(prefix.length));
	}
	return null;
}

function isSameOrigin(url: string): boolean {
	// Relative URLs are same-origin.
	if (!url.startsWith("http://") && !url.startsWith("https://")) return true;
	try {
		const u = new URL(url);
		return u.origin === window.location.origin;
	} catch {
		return false;
	}
}

if (typeof window !== "undefined") {
	const originalFetch = window.fetch.bind(window);
	window.fetch = async (input, init) => {
		const method = (init?.method ?? (input instanceof Request ? input.method : "GET")).toUpperCase();
		if (!MUTATING_METHODS.has(method)) {
			return originalFetch(input, init);
		}
		const url = typeof input === "string" ? input : input instanceof URL ? input.toString() : input.url;
		if (!isSameOrigin(url)) {
			return originalFetch(input, init);
		}
		const token = readCsrfCookie();
		if (!token) {
			// No cookie yet — let the request go; on a cold start the server
			// mints a token on the first response and subsequent calls succeed.
			return originalFetch(input, init);
		}
		// Build a new init with the CSRF header added, preserving anything
		// the caller already set.
		const headers = new Headers(init?.headers ?? (input instanceof Request ? input.headers : undefined));
		if (!headers.has(CSRF_HEADER)) {
			headers.set(CSRF_HEADER, token);
		}
		const merged: RequestInit = { ...(init ?? {}), headers };
		// If `input` is a Request, we have to pass the URL string + merged
		// init so the headers actually apply.
		if (input instanceof Request) {
			return originalFetch(input.url, { ...merged, method, body: init?.body ?? (await input.clone().blob()) });
		}
		return originalFetch(input, merged);
	};
}
