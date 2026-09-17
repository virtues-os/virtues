/**
 * Getting started — the derived state of the one room, for everything that
 * renders it: the room's mast and cards, the sidebar's progress card, the
 * door, the composer's lock. Nothing here is progress; it is a mirror of
 * `GET /api/getting-started`, which derives every step from rows on each
 * read. `refresh()` after anything that could flip a step (a card's write,
 * a source connecting, a skip) — the server has nothing to push.
 *
 * A 404 reads as "an older box": nothing to show. A phone that
 * updated ahead of its server must never strand on a missing endpoint.
 */
import {
	getGettingStarted,
	skipGettingStartedStep,
	startGettingStartedInterview,
	type GettingStartedState,
	type GettingStartedStepId,
} from "$lib/api/client";

const POLL_MS = 30_000;

class GettingStartedStore {
	state = $state<GettingStartedState | null>(null);
	loaded = $state(false);
	/** The endpoint is missing (an older box): behave as done. */
	unsupported = $state(false);
	private timer: ReturnType<typeof setInterval> | null = null;
	private inflight: Promise<void> | null = null;

	get aiConnected(): boolean {
		return this.state?.ai_connected ?? true;
	}
	get graduated(): boolean {
		return this.state?.graduated ?? true;
	}
	get steps() {
		return this.state?.steps ?? [];
	}
	get openCount(): number {
		return this.steps.filter((s) => s.status === "open").length;
	}
	step(id: GettingStartedStepId) {
		return this.steps.find((s) => s.id === id) ?? null;
	}
	/** The first open step: the card that opens by default. */
	get firstOpen(): GettingStartedStepId | null {
		return this.steps.find((s) => s.status === "open")?.id ?? null;
	}

	refresh(): Promise<void> {
		if (this.inflight) return this.inflight;
		this.inflight = (async () => {
			try {
				this.state = await getGettingStarted();
				this.unsupported = false;
			} catch (e) {
				// Only a 404 means unsupported; a blip keeps the last state.
				if (e instanceof Error && /404|Not Found/i.test(e.message)) {
					this.unsupported = true;
					this.state = null;
				}
			} finally {
				this.loaded = true;
				this.inflight = null;
			}
		})();
		return this.inflight;
	}

	async skip(step: GettingStartedStepId, skipped = true): Promise<void> {
		this.state = await skipGettingStartedStep(step, skipped);
	}

	/**
	 * Set by `startInterview` and consumed by the room once the opening is
	 * in the thread: the opening is authored, not streamed, so the room
	 * reveals it the way a turn arrives — but only on the press of Start,
	 * never on a reload of a thread that already holds it.
	 */
	revealOpening = $state(false);

	/** The interview begins, inside the room. */
	async startInterview(): Promise<void> {
		this.state = await startGettingStartedInterview();
		this.revealOpening = true;
	}

	/** The interview is the conversation now: begun, and no document yet. */
	get interviewUnderway(): boolean {
		return !!this.state?.interview_started_at && this.step("interview")?.status !== "done";
	}

	/** Poll while anything is open: sources land on cron, the interview
	 *  closes in another room, a phone pairs from elsewhere. Stops itself at
	 *  graduation; `start()` again revives it. */
	start(): void {
		void this.refresh();
		if (this.timer) return;
		this.timer = setInterval(() => {
			if (typeof document !== "undefined" && document.hidden) return;
			if (this.graduated && this.loaded) {
				this.stop();
				return;
			}
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

export const gettingStarted = new GettingStartedStore();
