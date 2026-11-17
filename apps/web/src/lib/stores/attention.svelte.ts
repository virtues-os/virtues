/**
 * What needs the owner, from `GET /api/attention`: the list home's notes
 * line reads (HomeNotes).
 *
 * Every item is a condition the server observes (a stream stopped, the
 * backup is missing). It stays while the condition holds and goes when the
 * server sees it fixed, so nothing here is dismissed: the list is simply
 * re-read.
 */
import { getAttention, type AttentionItem } from '$lib/api/client';

const POLL_MS = 60_000;

class AttentionStore {
	items = $state<AttentionItem[]>([]);
	loaded = $state(false);
	private timer: ReturnType<typeof setInterval> | null = null;
	/** Views showing the list now. The poll runs while there is one. */
	private users = 0;
	private inflight: Promise<void> | null = null;

	refresh(): Promise<void> {
		if (this.inflight) return this.inflight;
		this.inflight = (async () => {
			try {
				this.items = await getAttention();
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

	/** Poll while a view shows the list and the window is showing. The
	 *  conditions change on the server's clock (a stream goes quiet
	 *  overnight), not on a click here. Each `start` is matched by a `stop`
	 *  when its view goes: the poll used to run for the life of the window
	 *  once home had been shown, and each poll scans every stream. */
	start(): void {
		this.users++;
		void this.refresh();
		if (this.timer) return;
		this.timer = setInterval(() => {
			if (typeof document !== 'undefined' && document.hidden) return;
			void this.refresh();
		}, POLL_MS);
	}

	stop(): void {
		this.users = Math.max(0, this.users - 1);
		if (this.users > 0) return;
		if (this.timer) {
			clearInterval(this.timer);
			this.timer = null;
		}
	}
}

export const attention = new AttentionStore();
