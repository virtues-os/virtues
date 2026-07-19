// The SPA's own build identity — {version, sha, channel} baked at build time by
// Vite (see vite.config.ts) and mirroring the box's build.rs/codename.rs so every
// artifact reports the same shape. This is what "this browser" sends to the box in
// the X-Virtues-Client header and shows on the Devices page.
// See docs/update-identity-spine.md.

export type Channel = 'stable' | 'staging' | 'edge' | 'dev';

export interface BuildInfo {
	/** Clean version, e.g. "0.2.0" or "0.1.0-staging.57" or "dev". */
	version: string;
	/** Short git commit, e.g. "13cfd9c", or "dev" for untagged local builds. */
	sha: string;
	channel: Channel;
}

/** Derive the release channel from the raw build tag — same rules as the box's
 *  codename::channel(): bare vX.Y.Z → stable; anything with `staging`/`edge`/an
 *  offset → the matching prerelease track; empty/dev → dev. */
function deriveChannel(rawVersion: string): Channel {
	if (!rawVersion || rawVersion === 'dev') return 'dev';
	if (rawVersion.includes('staging')) return 'staging';
	if (rawVersion.startsWith('edge')) return 'edge';
	if (rawVersion.includes('-')) return 'dev'; // e.g. v0.2.0-4-gabc123 / -dirty
	return 'stable';
}

const rawVersion = typeof __BUILD_VERSION__ !== 'undefined' ? __BUILD_VERSION__ : 'dev';
const rawSha = typeof __BUILD_COMMIT__ !== 'undefined' ? __BUILD_COMMIT__ : 'dev';

export const BUILD: BuildInfo = {
	version: rawVersion.replace(/^v/, ''),
	sha: rawSha === 'dev' ? 'dev' : rawSha.slice(0, 7),
	channel: deriveChannel(rawVersion)
};

/** Compact one-line identity for display, e.g. "0.2.0 (13cfd9c) · stable". */
export function buildLabel(b: BuildInfo = BUILD): string {
	const sha = b.sha && b.sha !== 'dev' ? ` (${b.sha})` : '';
	return `${b.version}${sha} · ${b.channel}`;
}

/** The value for the X-Virtues-Client request header. */
export function clientHeader(b: BuildInfo = BUILD): string {
	return `version=${b.version}; sha=${b.sha}; channel=${b.channel}`;
}

/**
 * Install a one-time global fetch interceptor that stamps `X-Virtues-Client` on
 * same-origin box requests, so the box can record this browser's build on its
 * device row (shown on the Devices page). Idempotent and SSR-safe. Only touches
 * string/URL inputs (the codebase's `fetch(url, init)` pattern) — Request-object
 * calls pass through untouched to avoid body/stream re-wrapping hazards.
 */
export function installClientHeader(): void {
	if (typeof window === 'undefined') return;
	const w = window as unknown as { __virtuesFetchPatched?: boolean };
	if (w.__virtuesFetchPatched) return;
	w.__virtuesFetchPatched = true;

	const orig = window.fetch.bind(window);
	const header = clientHeader();
	window.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
		if (typeof input === 'string' || input instanceof URL) {
			const url = typeof input === 'string' ? input : input.href;
			if (url.startsWith('/') || url.startsWith(window.location.origin)) {
				const headers = new Headers(init?.headers);
				if (!headers.has('X-Virtues-Client')) headers.set('X-Virtues-Client', header);
				return orig(input, { ...init, headers });
			}
		}
		return orig(input, init);
	};
}
