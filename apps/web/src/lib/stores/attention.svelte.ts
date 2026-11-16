/**
 * What needs the owner, from `GET /api/attention`: the list behind the
 * status capsule at the right of the window header (StatusCapsule).
 *
 * Every item is a condition the server observes (a stream stopped, the
 * backup is missing). It stays while the condition holds and goes when the
 * server sees it fixed, so nothing here is dismissed: the list is simply
 * re-read.
 *
 * What you have already looked at is remembered in this browser (`seen`),
 * so the capsule's mark lights only for something new. Opening the list
 * marks everything in it seen. A key is forgotten once its item leaves the
 * list, so a condition that comes back later lights the mark again.
 */
import { getAttention, type AttentionItem } from '$lib/api/client';

const POLL_MS = 60_000;
const SEEN_KEY = 'virtues-attention-seen';

function loadSeen(): string[] {
	try {
		const saved = JSON.parse(localStorage.getItem(SEEN_KEY) ?? '[]');
		return Array.isArray(saved) ? saved.filter((k) => typeof k === 'string') : [];
	} catch {
		return [];
	}
}

function saveSeen(keys: string[]): void {
	try {
		localStorage.setItem(SEEN_KEY, JSON.stringify(keys));
	} catch {
		// The marks still hold until the app reloads.
	}
}

class AttentionStore {
	items = $state<AttentionItem[]>([]);
	loaded = $state(false);
	/** Keys of the items you have looked at. */
	seen = $state<string[]>(loadSeen());
	/** What is in the list that you have not looked at yet. */
	unseen = $derived(this.items.filter((item) => !this.seen.includes(item.key)));
	private timer: ReturnType<typeof setInterval> | null = null;
	private inflight: Promise<void> | null = null;

	refresh(): Promise<void> {
		if (this.inflight) return this.inflight;
		this.inflight = (async () => {
			try {
				this.items = await getAttention();
				this.forgetGone();
			} catch (e) {
				// A blip keeps the last list. An unreachable server has its
				// own bar; the inbox does not guess at what it can't read.
				console.warn('[attention] could not read the list:', e);
			} finally {
				this.loaded = true;
				this.inflight = null;
			}
		})();
		return this.inflight;
	}

	/** You opened the list: everything in it has been seen. */
	markSeen(): void {
		const keys = this.items.map((item) => item.key);
		if (keys.every((k) => this.seen.includes(k)) && keys.length === this.seen.length) return;
		this.seen = keys;
		saveSeen(keys);
	}

	/** Drop the keys of items the server no longer lists. */
	private forgetGone(): void {
		const kept = this.seen.filter((k) => this.items.some((item) => item.key === k));
		if (kept.length === this.seen.length) return;
		this.seen = kept;
		saveSeen(kept);
	}

	/** Poll while the window is showing. The conditions change on the
	 *  server's clock (a stream goes quiet overnight), not on a click here. */
	start(): void {
		void this.refresh();
		if (this.timer) return;
		this.timer = setInterval(() => {
			if (typeof document !== 'undefined' && document.hidden) return;
			void this.refresh();
		}, POLL_MS);
	}

	stop(): void {
		if (this.timer) {
			clearInterval(this.timer);
			this.timer = null;
		}
	}
}

export const attention = new AttentionStore();
