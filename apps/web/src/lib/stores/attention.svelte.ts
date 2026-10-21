/**
 * What needs the owner, from `GET /api/attention`: the inbox behind the ∴
 * mark in the window header.
 *
 * Every item is a condition the server observes (a stream stopped, the
 * backup is missing). It stays while the condition holds and goes when the
 * server sees it fixed, so nothing here is read or dismissed: the list is
 * simply re-read.
 */
import { getAttention, type AttentionItem } from '$lib/api/client';

const POLL_MS = 60_000;

class AttentionStore {
	items = $state<AttentionItem[]>([]);
	loaded = $state(false);
	private timer: ReturnType<typeof setInterval> | null = null;
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
