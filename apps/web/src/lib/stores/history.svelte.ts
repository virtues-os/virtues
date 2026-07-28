/**
 * Sidebar "Recents" — the read side of `app_history`.
 *
 * Writes don't go through here: `windowShellStore` records visits directly at
 * the point navigation converges. This store only reads, filters, and forgets.
 *
 * The filter is persisted because it's a standing preference, not a transient
 * one — someone who only ever wants to see pages in Recents wants that next
 * week too, and having to re-set it every session would make the filter useless.
 */

import {
	listHistory,
	clearHistory,
	forgetHistoryUrl,
	type HistoryEntry,
} from '$lib/api/client';

/** Buckets offered in the filter menu, in display order. */
export const HISTORY_KINDS = [
	{ id: 'chat', label: 'Chats' },
	{ id: 'page', label: 'Pages' },
	{ id: 'notebook', label: 'Notebooks' },
	{ id: 'record', label: 'Records' },
	{ id: 'wiki', label: 'Wiki' },
] as const;

const FILTER_STORAGE_KEY = 'virtues-recents-filter';
const DEFAULT_LIMIT = 12;

interface PersistedFilter {
	kinds: string[];
	limit: number;
}

function loadFilter(): PersistedFilter {
	if (typeof localStorage === 'undefined') return { kinds: [], limit: DEFAULT_LIMIT };
	try {
		const raw = localStorage.getItem(FILTER_STORAGE_KEY);
		if (!raw) return { kinds: [], limit: DEFAULT_LIMIT };
		const parsed = JSON.parse(raw) as Partial<PersistedFilter>;
		return {
			kinds: Array.isArray(parsed.kinds) ? parsed.kinds : [],
			limit: typeof parsed.limit === 'number' ? parsed.limit : DEFAULT_LIMIT,
		};
	} catch {
		return { kinds: [], limit: DEFAULT_LIMIT };
	}
}

class HistoryStore {
	entries = $state<HistoryEntry[]>([]);
	loading = $state(false);
	/** Empty means "everything" — the common case, so it's the default. */
	kinds = $state<string[]>(loadFilter().kinds);
	limit = $state<number>(loadFilter().limit);

	async load(): Promise<void> {
		this.loading = true;
		try {
			this.entries = await listHistory({
				kinds: this.kinds.length ? this.kinds : undefined,
				limit: this.limit,
			});
		} catch (err) {
			console.error('[history] load failed:', err);
			this.entries = [];
		} finally {
			this.loading = false;
		}
	}

	/** Toggle one kind in the filter, persist, and reload. */
	async toggleKind(kind: string): Promise<void> {
		this.kinds = this.kinds.includes(kind)
			? this.kinds.filter((k) => k !== kind)
			: [...this.kinds, kind];
		this.persist();
		await this.load();
	}

	async setLimit(limit: number): Promise<void> {
		this.limit = limit;
		this.persist();
		await this.load();
	}

	async clearFilter(): Promise<void> {
		this.kinds = [];
		this.persist();
		await this.load();
	}

	/** Drop one destination from history, optimistically. */
	async forget(url: string): Promise<void> {
		this.entries = this.entries.filter((e) => e.url !== url);
		try {
			await forgetHistoryUrl(url);
		} catch (err) {
			console.error('[history] forget failed:', err);
			await this.load();
		}
	}

	async clearAll(): Promise<void> {
		this.entries = [];
		try {
			await clearHistory();
		} catch (err) {
			console.error('[history] clear failed:', err);
			await this.load();
		}
	}

	private persist(): void {
		if (typeof localStorage === 'undefined') return;
		localStorage.setItem(
			FILTER_STORAGE_KEY,
			JSON.stringify({ kinds: this.kinds, limit: this.limit }),
		);
	}
}

export const historyStore = new HistoryStore();
