/**
 * openingReveal — the getting-started interview's opening, arriving as a turn does.
 *
 * Three authored messages revealed at a quick model's pace through the same
 * Streamdown path a live stream takes, so they fade in word by word and the
 * table builds as it goes — in order, with a breath between them; the plate
 * waits for its heading. `null` = nothing revealing, everything shown.
 *
 * Only ever started by the press of Start (the store's flag): a thread reloaded
 * with the opening already in it shows it whole, as it should.
 */

import { GS_INTERVIEW_OPENING_ID } from "$lib/components/chat/getting-started/getting-started";

const OPENING_IDS = [GS_INTERVIEW_OPENING_ID, "gs-iv-body", "gs-iv-ask"];

type OpeningMessage = { id: string; parts: unknown[] };

export class OpeningRevealController {
	/** Characters shown per message id, or null when nothing is revealing. */
	chars = $state<Record<string, number> | null>(null);

	#timer: ReturnType<typeof setTimeout> | null = null;
	#messages: () => OpeningMessage[];

	constructor(messages: () => OpeningMessage[]) {
		this.#messages = messages;
	}

	#textOf(id: string): string {
		const m = this.#messages().find((x) => x.id === id);
		return ((m?.parts?.[0] as { text?: string } | undefined)?.text ?? "");
	}

	/** Is the opening plate in this thread at all? */
	hasOpening(): boolean {
		return this.#messages().some((m) => m.id === GS_INTERVIEW_OPENING_ID);
	}

	start() {
		if (this.#timer) clearTimeout(this.#timer);
		if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
		const texts = OPENING_IDS.map((id) => this.#textOf(id));
		const shown: Record<string, number> = Object.fromEntries(OPENING_IDS.map((id) => [id, 0]));
		this.chars = { ...shown };
		const RATE = 420; // characters per second: a quick model, not a typewriter
		const BREATH = 260; // ms between one message and the next
		let i = 0;
		let start = performance.now();
		const tick = (now: number) => {
			const id = OPENING_IDS[i];
			const full = texts[i].length;
			shown[id] = Math.max(0, Math.min(full, Math.floor(((now - start) / 1000) * RATE)));
			this.chars = { ...shown };
			if (shown[id] >= full) {
				i += 1;
				if (i >= OPENING_IDS.length) {
					this.chars = null;
					return;
				}
				start = now + BREATH;
			}
			// A timer, not rAF: the pace is elapsed-time, so frame sync buys
			// nothing, and rAF stops in a background tab — someone who presses
			// Start and glances at another tab would come back to a reveal
			// frozen at nothing. A throttled timer just takes coarser steps.
			this.#timer = setTimeout(() => tick(performance.now()), 16);
		};
		tick(performance.now());
	}

	stop() {
		if (this.#timer) clearTimeout(this.#timer);
		this.#timer = null;
	}

	/** How much of a text part to show right now, and whether it is still arriving. */
	revealed(messageId: string, text: string): { content: string; arriving: boolean } {
		const n = this.chars?.[messageId];
		if (n === undefined) return { content: text, arriving: false };
		return { content: text.slice(0, n), arriving: n < text.length };
	}

	/** The plate under the opening's heading waits for the heading. */
	readonly plateReady = $derived.by(() => {
		if (this.chars === null) return true;
		const text = this.#textOf(GS_INTERVIEW_OPENING_ID);
		return (this.chars[GS_INTERVIEW_OPENING_ID] ?? 0) >= text.length;
	});
}
