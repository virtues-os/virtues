// The SPA's own build identity — {version, sha, channel} baked at build time by
// Vite (see vite.config.ts) and mirroring the box's build.rs/codename.rs so every
// artifact reports the same shape. This is what "this browser" sends to the box in
// the X-Virtues-Client header and shows on the Devices page.
// See agents/record/update-identity-spine.md.

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

/** The native shell's own release (`1.0.23`), once known. The SPA's
 *  version/sha/channel describe the UI BUNDLE — which, for a paired desktop,
 *  is the box-served SPA and therefore mirrors the box. Only this field lets
 *  the box's Devices page answer "which app binary is that device on", so the
 *  layout feeds it in as soon as the shell bridge resolves. Stays null in a
 *  plain browser. */
let shellAppVersion: string | null = null;

export function setShellAppVersion(v: string): void {
	if (!v) return;
	shellAppVersion = v;
	currentHeader = clientHeader();
}

/** The value for the X-Virtues-Client request header. */
export function clientHeader(b: BuildInfo = BUILD): string {
	const app = shellAppVersion ? `; app=${shellAppVersion}` : '';
	return `version=${b.version}; sha=${b.sha}; channel=${b.channel}${app}`;
}

/** Cached header value; recomputed only when the shell identity arrives. */
let currentHeader = clientHeader();

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
	window.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
		if (typeof input === 'string' || input instanceof URL) {
			const url = typeof input === 'string' ? input : input.href;
			if (url.startsWith('/') || url.startsWith(window.location.origin)) {
				const headers = new Headers(init?.headers);
				// `currentHeader`, not a captured snapshot: the shell's app
				// version arrives after install, and a frozen header would
				// report every request as app-less forever.
				if (!headers.has('X-Virtues-Client')) headers.set('X-Virtues-Client', currentHeader);
				return orig(input, { ...init, headers }).then((r) => {
					noteBoxBuild(r);
					return r;
				});
			}
		}
		return orig(input, init);
	};
}

// ── Staleness watch ─────────────────────────────────────────────────────────
// A box-served page outlives the box build that served it: after an upgrade
// the flipped web/ slot no longer holds this page's content-hashed chunks, so
// the first lazy navigation 404s — and until now nothing told any open page
// (other tabs, the desktop webview) to reload; only the tab that pressed the
// update button did. The box now stamps every response with its build
// (`X-Virtues-Box-Build`); when it moves under us, reload — but only from the
// background, never out from under someone mid-thought. The always-visible
// kiosk has its own recovery (`restart_display`), and the bundled mobile SPA
// serves its chunks locally (a box upgrade cannot strand it), so this guards
// exactly the surfaces that had none.
//
// WHAT THIS CAN AND CANNOT SEE. `X-Virtues-Box-Build` is stamped from the
// BINARY's compiled-in commit, once, into a OnceLock — so it moves when the box
// process is replaced and at no other time. `virtues upgrade --only web`
// refreshes the served build with no binary swap and no restart
// (`cli/upgrade.rs`), which flips exactly the web/ slot this guard exists to
// notice, and moves nothing this guard reads. That blind spot is covered
// elsewhere, by the sidebar chip polling SvelteKit's own `/_app/version.json`;
// this stays as the fast path for the common case, a box that restarted.

let boxBuild: string | null = null;
let reloadArmed = false;
const boxMovedListeners = new Set<() => void>();

/**
 * Run `cb` when the box's build changes under this page; returns an unsubscribe.
 *
 * The reload below only ever happens once the page is hidden, which is right —
 * nobody should have a page reloaded out from under them mid-sentence — but it
 * leaves a page that STAYS visible with no signal at all until something else
 * notices. This is that something else: the sidebar chip re-checks on the spot
 * instead of waiting out its ten-minute poll.
 */
export function onBoxBuildChanged(cb: () => void): () => void {
	boxMovedListeners.add(cb);
	return () => boxMovedListeners.delete(cb);
}

function noteBoxBuild(r: Response): void {
	if (window.location.protocol === 'virtues:') return; // bundled SPA — chunks are local
	const b = r.headers.get('x-virtues-box-build');
	if (!b) return;
	if (boxBuild === null) {
		boxBuild = b;
		return;
	}
	if (b === boxBuild) return;
	boxBuild = b;
	for (const cb of boxMovedListeners) {
		// One listener throwing must not cost the others their notification,
		// nor take down the fetch whose response this is riding.
		try {
			cb();
		} catch {
			/* a subscriber's problem, not this page's */
		}
	}
	if (reloadArmed) return;
	reloadArmed = true;
	if (document.hidden) {
		window.location.reload();
	} else {
		const onHide = () => {
			if (!document.hidden) return; // becoming visible is not the moment
			document.removeEventListener('visibilitychange', onHide);
			window.location.reload();
		};
		document.addEventListener('visibilitychange', onHide);
	}
}
