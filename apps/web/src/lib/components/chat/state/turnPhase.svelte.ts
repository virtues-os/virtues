/**
 * Whether a turn is working, and for how long — one answer for the whole view.
 *
 * Eighteen expressions in ChatView used to answer "is a turn in flight", and
 * they disagreed: the thinking block counted only `streaming`, so nothing
 * showed during `submitted` after the first turn; Stop was offered only on
 * `streaming`, so a slow start could not be cancelled; a send-in-progress
 * flag outlived the stream by the length of a title request; the duration
 * was one number for the whole view and leaked into the next chat opened.
 * Each indicator reads this instead.
 *
 * `working` is from the moment Send is pressed (before the SDK has moved,
 * while permissions sync) until the stream ends. `stalled` is working with
 * nothing arriving: the label says so, and past LET_GO_MS the view lets the
 * dead stream go and asks to rejoin — the box's turn outlives the request.
 */

import { untrack } from "svelte";

/** No chunk for this long and the turn reads as waiting on the server. */
const STALL_MS = 45_000;
/** No chunk for this long and the stream is presumed dead (the box's own
 *  idle timeout is 300s, so a live turn has spoken well before this). */
export const LET_GO_MS = 300_000;

type Status = "submitted" | "streaming" | "ready" | "error";

interface ChatLike {
	status: Status;
	messages: { id: string; role: string }[];
}

export class TurnPhaseController {
	/** Send pressed, the SDK not yet moved (permissions sync, file reads). */
	private sending = $state(false);
	/** When the turn on screen began, by this client's clock. */
	startedAt = $state<number | null>(null);
	/** Working, and nothing has arrived for STALL_MS. */
	stalled = $state(false);
	/** Turns watched to the end here: assistant message id → seconds. */
	private measured = $state(new Map<string, number>());
	private lastChunkAt = 0;

	constructor(
		private chat: () => ChatLike | null | undefined,
		private onLetGo: () => void,
	) {
		// Start and end. Keyed off `working`, so a rejoin's turn is timed from
		// when this client attached — the server span covers the rest.
		$effect(() => {
			const working = this.working;
			const startedAt = untrack(() => this.startedAt);
			if (working && startedAt === null) {
				this.startedAt = Date.now();
				this.lastChunkAt = Date.now();
			} else if (!working && startedAt !== null) {
				untrack(() => {
					const last = this.chat()?.messages.at(-1);
					if (last?.role !== "assistant") return;
					const next = new Map(this.measured);
					next.set(last.id, (Date.now() - startedAt) / 1000);
					this.measured = next;
				});
				this.startedAt = null;
				this.stalled = false;
			}
		});

		// The SDK has the turn: the send flag has done its job.
		$effect(() => {
			const s = this.chat()?.status;
			if (s === "submitted" || s === "streaming") this.sending = false;
		});

		// A chunk is a write to the last message, which the SDK replaces.
		$effect(() => {
			void this.chat()?.messages.at(-1);
			this.lastChunkAt = Date.now();
			this.stalled = false;
		});

		$effect(() => {
			if (!this.working) return;
			const id = setInterval(() => {
				const quiet = Date.now() - this.lastChunkAt;
				this.stalled = quiet > STALL_MS;
				if (quiet > LET_GO_MS) {
					this.lastChunkAt = Date.now();
					this.onLetGo();
				}
			}, 5_000);
			return () => clearInterval(id);
		});
	}

	/** A turn is in flight: from Send to the stream's end. */
	get working(): boolean {
		const s = this.chat()?.status;
		return this.sending || s === "submitted" || s === "streaming";
	}

	/** In flight, and the reply's message does not exist yet. */
	get awaitingReply(): boolean {
		return this.working && this.chat()?.messages.at(-1)?.role !== "assistant";
	}

	/** Seconds the turn that wrote this message took, if it was watched here. */
	secondsFor(messageId: string): number {
		return this.measured.get(messageId) ?? 0;
	}

	beginSend() {
		this.sending = true;
	}

	endSend() {
		this.sending = false;
	}

	/** A different conversation: nothing measured here belongs to it. */
	reset() {
		this.sending = false;
		this.startedAt = null;
		this.stalled = false;
		this.measured = new Map();
	}
}
