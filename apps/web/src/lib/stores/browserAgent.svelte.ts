/**
 * The assistant in the owner's Browser, as the shell reports it: who is
 * driving, what the assistant has asked the owner to do, and the steps it took.
 * One store, fed once (`browserPaneListener.ts`), so a Browser tab that moves
 * or remounts keeps the log.
 */
import type { BrowserAgentState, BrowserStepEvent } from '$lib/tauri/bridge';

export interface BrowserStep extends BrowserStepEvent {
	id: number;
	at: number;
}

/** Enough to scroll back through one task; the thumbnails are ~30 KB each. */
const MAX_STEPS = 60;

class BrowserAgentStore {
	driving = $state(false);
	paused = $state(false);
	handoff = $state<string | null>(null);
	steps = $state<BrowserStep[]>([]);
	#next = 1;

	setState(state: BrowserAgentState) {
		this.driving = state.driving;
		this.paused = state.paused;
		this.handoff = state.handoff;
	}

	addStep(step: BrowserStepEvent) {
		const next = [...this.steps, { ...step, id: this.#next++, at: Date.now() }];
		this.steps = next.length > MAX_STEPS ? next.slice(-MAX_STEPS) : next;
	}

	clearSteps() {
		this.steps = [];
	}
}

export const browserAgent = new BrowserAgentStore();
