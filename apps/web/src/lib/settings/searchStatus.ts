/**
 * The words on Settings · Search: the sentence it opens with, where search
 * runs, and the model's name.
 *
 * The opening sentence says the first of: search isn't answering, it's busy,
 * it's moving to a new model, it's answering. Under it, how much of your
 * record the index covers.
 */
import type { SearchStatus } from '$lib/api/client';
import { whenInWords } from '$lib/utils/dateUtils';

/** The host of a URL, or the URL itself when it doesn't parse. */
export function hostOf(url: string): string {
	try {
		return new URL(url).host;
	} catch {
		return url;
	}
}

/** The CPU engine Virtues runs, on its usual port or (mid-move) the next one. */
function isOwnEngine(url: string): boolean {
	return /^http:\/\/127\.0\.0\.1:1818[13]$/.test(url);
}

/** A server outside your network isn't necessarily yours, so it is named as what it is. */
function otherServer(s: SearchStatus): string {
	const host = hostOf(s.embed_url);
	return s.embed_locality === 'public' ? `${host}, outside your network` : `Your server at ${host}`;
}

/** The "Runs on" value: this server's processor, or another machine by its host. */
export function runsOn(s: SearchStatus): string {
	// Going back to the recommended setup: search uses the other server until
	// the new index is ready.
	if (s.mode === 'bundled' && !isOwnEngine(s.embed_url)) return otherServer(s);
	switch (s.mode) {
		case 'bundled':
			return 'This server’s processor';
		case 'manual':
			return otherServer(s);
		default:
			return s.embed_url;
	}
}

/** The model's name, without the folders or the file extension an embedding server may report. */
export function modelName(id: string): string {
	return id.split('/').pop()!.replace(/\.gguf$/i, '') || id;
}

/** How far a move to another model has come, 0 to 99. */
export function changePercent(s: SearchStatus): number {
	const c = s.model_change;
	if (!c || c.total === 0) return 0;
	return Math.min(99, Math.floor((c.done / c.total) * 100));
}

/** What the index covers: "It covers 1,200 records, and your server last added to it 4 minutes ago." */
function coverage(s: SearchStatus, now: number): string {
	if (s.records_searchable === 0) return 'Nothing is in the index yet.';
	const records = `It covers ${s.records_searchable.toLocaleString()} record${s.records_searchable === 1 ? '' : 's'}`;
	return s.last_indexed_at ? `${records}, and your server last added to it ${whenInWords(s.last_indexed_at, now)}.` : `${records}.`;
}

export function searchLead(s: SearchStatus, now = Date.now()): { sentence: string; sub: string } {
	const host = hostOf(s.embed_url);
	if (s.busy) {
		return {
			sentence: 'Search is busy.',
			sub: `The server at ${host} took more than 5 seconds to answer. It’s usually busy indexing, and searches wait their turn.`,
		};
	}
	if (!s.reachable) {
		return {
			sentence: 'Search isn’t answering.',
			sub: `New searches can’t run until the server at ${host} answers. Check that it’s running, then check again.`,
		};
	}
	if (s.model_change) {
		return {
			sentence: 'Search is moving to a new model.',
			sub: `${changePercent(s)}% of the index is rebuilt for ${modelName(s.model_change.model)}. Search keeps using the current model until the rebuild finishes.`,
		};
	}
	return { sentence: 'Search is answering.', sub: coverage(s, now) };
}
