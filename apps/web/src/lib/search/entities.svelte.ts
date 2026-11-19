/**
 * People and places, for search to find by name. The wiki's own lists are
 * the source; search loads them when it opens, at most once a minute, and
 * matches them on the device like chats and pages, so a name is found
 * without a round trip per keystroke.
 */
import { listPeople, listPlaces } from '$lib/wiki/api';
import { matchRank } from '$lib/search/match';

export interface EntityRow {
	kind: 'person' | 'place';
	id: string;
	name: string;
	url: string;
	/** Records that mention it, the tie-break between equal matches. */
	refs: number;
}

let rows = $state<EntityRow[]>([]);
let loadedAt = 0;
let loading: Promise<void> | null = null;

export const entities = {
	get rows(): EntityRow[] {
		return rows;
	},
	/** Fetch the lists, unless they were fetched in the last minute. */
	load(): Promise<void> {
		if (loading) return loading;
		if (Date.now() - loadedAt < 60_000) return Promise.resolve();
		loading = Promise.all([listPeople(), listPlaces()])
			.then(([people, places]) => {
				rows = [
					...people.map((p) => ({ kind: 'person' as const, id: p.id, name: p.name, url: `/person/${p.id}`, refs: p.ref_count })),
					...places.map((p) => ({ kind: 'place' as const, id: p.id, name: p.name, url: `/place/${p.id}`, refs: p.ref_count })),
				];
				loadedAt = Date.now();
			})
			.finally(() => (loading = null));
		return loading;
	},
};

/** People and places whose names match `text`, best match first, then the most mentioned. */
export function matchEntities(text: string, limit: number): EntityRow[] {
	if (text.length < 2) return [];
	return rows
		.filter((row) => row.name.trim())
		.map((row) => ({ row, rank: matchRank(row.name, text) }))
		.filter((x) => x.rank >= 2)
		.sort((a, b) => b.rank - a.rank || b.row.refs - a.row.refs)
		.slice(0, limit)
		.map((x) => x.row);
}
