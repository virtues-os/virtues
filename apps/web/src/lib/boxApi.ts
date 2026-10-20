/**
 * Is the server new enough for this copy of the app?
 *
 * The box reports one integer, `api_version` in `/health`
 * (virtues-core/src/api_version.rs), raised whenever it gains something a UI
 * may depend on. A box that predates the field reports nothing, read as 0.
 *
 * This copy of the app declares the lowest one it can run against:
 * `MIN_BOX_API`. Below it, the app shows one screen, "Your server needs an
 * update" (`/server-update`), rather than running half-broken. Raise it only
 * deliberately, with a release note: every box below it stops opening this
 * app until it updates. See agents/record/local-ui.md.
 *
 * It is a constant in code, not a bundle-manifest field, on purpose: a bundle
 * the box pushes over the air always matches that box, so only a copy baked
 * into an app can be ahead of its server, and that is a question for the
 * running app.
 *
 * Individual features gate on `boxApiVersion()` one at a time, as each
 * "a 404 means an older box" guess is replaced.
 */

/**
 * The lowest `api_version` this app runs against: 4, the box that holds a
 * page in blocks under document contract 2. This app edits only block
 * pages, and a block editor at contract 2 on a box below it would open
 * every page read only, or edit none of a box's markdown pages at all, so
 * an older box updates first.
 */
export const MIN_BOX_API = 4;

let cached: Promise<number | null> | null = null;

/**
 * The box's `api_version`: 0 when it is too old to report one, `null` when it
 * could not be asked (unreachable). Cached for the page; `fresh` asks again,
 * for waiting out an update.
 */
export function boxApiVersion(fetchFn: typeof fetch = fetch, fresh = false): Promise<number | null> {
	if (!cached || fresh) {
		cached = (async () => {
			try {
				const res = await fetchFn('/health', { cache: 'no-store' });
				if (!res.ok) return null;
				const body = (await res.json()) as { api_version?: unknown };
				return typeof body.api_version === 'number' ? body.api_version : 0;
			} catch {
				return null;
			}
		})();
	}
	return cached;
}

/**
 * True only when the box answered and is below this app's floor. An
 * unreachable box is not "too old": that is the reconnect screen's job, and
 * blocking on it would turn a blip into a wall.
 */
export async function boxBelowFloor(fetchFn: typeof fetch = fetch): Promise<boolean> {
	const v = await boxApiVersion(fetchFn);
	return v !== null && v < MIN_BOX_API;
}
