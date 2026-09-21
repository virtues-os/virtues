/**
 * Visits — what you keep coming back to.
 *
 * The window shell reports every activation of a tab through `note(route)`
 * (its URL sync is the one place all activations pass, and it already skips
 * restores on reload and deep links). This store decides what counts as a
 * visit and posts it to the box, which keeps the log (migration 0031) and
 * serves the frecency read that ⌘K ranks by.
 *
 * What counts:
 *   - a chat, page or project route (the three things a person makes);
 *   - reached by a gesture of theirs — the shell marks opens the assistant
 *     made on their behalf, and those are dropped;
 *   - held for a few seconds. Flicking through tabs is not visiting them.
 * A second activation of the same route inside a minute is the same visit.
 *
 * Nothing here is shown. No screen carries the count; the number only
 * orders. "Viewed 14 times" is the register this app refuses.
 */
import { getFrecency, recordVisit, type VisitKind } from '$lib/api/client';

/** How long a route has to stay active before it is a visit. */
const DWELL_MS = 3000;
/** Repeats of one route inside this window are one visit. */
const DEDUPE_MS = 60_000;
/** The frecency read is cached this long; ⌘K opens refresh it past this. */
const FRECENCY_TTL_MS = 60_000;

const ROUTE_KIND: [RegExp, VisitKind][] = [
	[/^\/chat\/([^/?#]+)/, 'chat'],
	[/^\/page\/([^/?#]+)/, 'page'],
	[/^\/project\/([^/?#]+)/, 'project'],
];

function parseVisit(route: string): { kind: VisitKind; id: string } | null {
	for (const [re, kind] of ROUTE_KIND) {
		const m = route.match(re);
		if (m) return { kind, id: decodeURIComponent(m[1]) };
	}
	return null;
}

class VisitsStore {
	/** `kind:id` → frecency score, from the box. */
	private scores = $state<Map<string, number>>(new Map());
	private fetchedAt = 0;
	private inflight: Promise<void> | null = null;

	private dwellTimer: ReturnType<typeof setTimeout> | null = null;
	private lastNoted = new Map<string, number>();

	/** The shell calls this on every activation. */
	note(route: string | null | undefined, programmatic = false) {
		if (this.dwellTimer) {
			clearTimeout(this.dwellTimer);
			this.dwellTimer = null;
		}
		if (!route || programmatic) return;
		const visit = parseVisit(route);
		if (!visit) return;
		const key = `${visit.kind}:${visit.id}`;
		this.dwellTimer = setTimeout(() => {
			this.dwellTimer = null;
			const now = Date.now();
			const last = this.lastNoted.get(key) ?? 0;
			if (now - last < DEDUPE_MS) return;
			this.lastNoted.set(key, now);
			// Fire and forget: a lost visit is a lost visit. Logged, never surfaced.
			recordVisit(visit.kind, visit.id).catch((e) => {
				console.warn('[visits] not recorded:', e);
			});
		}, DWELL_MS);
	}

	/** Frecency for one record; 0 when never visited. */
	score(kind: VisitKind, id: string): number {
		return this.scores.get(`${kind}:${id}`) ?? 0;
	}

	/** Refresh the scores if they are stale. Safe to call on every ⌘K open. */
	refresh(): Promise<void> {
		if (this.inflight) return this.inflight;
		if (Date.now() - this.fetchedAt < FRECENCY_TTL_MS) return Promise.resolve();
		this.inflight = (async () => {
			try {
				const rows = await getFrecency();
				const next = new Map<string, number>();
				for (const r of rows) next.set(`${r.kind}:${r.record_id}`, r.score);
				this.scores = next;
				this.fetchedAt = Date.now();
			} catch (e) {
				// An older box has no endpoint; ⌘K then ranks by store order, as before.
				console.warn('[visits] frecency unavailable:', e);
				this.fetchedAt = Date.now();
			} finally {
				this.inflight = null;
			}
		})();
		return this.inflight;
	}
}

export const visits = new VisitsStore();
