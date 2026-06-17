/**
 * AI cursor session status — the source of truth for *UI* (the status-bar
 * "Virtues is writing…" indicator + stop button). Positions live in the
 * CodeMirror StateFields (ai-cursor.ts); this only tracks the phase.
 */

export type AiStatus =
	| "idle"
	| "thinking"
	| "telegraphing"
	| "streaming"
	| "done"
	| "error";

class AiSessionStore {
	status = $state<AiStatus>("idle");
	error = $state<string | null>(null);

	/** True while a session is mid-flight (thinking → streaming). */
	get active(): boolean {
		return (
			this.status === "thinking" ||
			this.status === "telegraphing" ||
			this.status === "streaming"
		);
	}

	set(status: AiStatus, error: string | null = null) {
		this.status = status;
		this.error = error;
	}

	reset() {
		this.status = "idle";
		this.error = null;
	}
}

export const aiSession = new AiSessionStore();
