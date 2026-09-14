/**
 * Getting started — the room after the founder's letter.
 *
 * One fixed conversation, seeded at boot; the server forces its mode by id
 * (see chat_handler) and refuses to delete or retitle it. Everything above
 * the first stored message is SYNTHETIC: the mast, one card per open step,
 * the promise line, the authored first line. They are rebuilt from the
 * derived state on every load and every state change, never persisted, so a
 * person back after a week sees today, not a replay of cards that no longer
 * apply. The interview room does the same with its opening
 * (chat/interview/interview.ts); this module is that pattern's second use.
 */
import type { Chat } from "@ai-sdk/svelte";
import type { GettingStartedState, GettingStartedStepId } from "$lib/api/client";

/** Mirrors getting_started::GETTING_STARTED_CHAT_ID on the server. */
export const GETTING_STARTED_CHAT_ID = "chat_getting_started";

/** Every synthetic message id starts with this; the room renders them as
 *  cards, and the opening strips and rebuilds them by it. */
export const GS_PREFIX = "gs-";
export const GS_MAST_ID = "gs-mast";
export const GS_PROMISE_ID = "gs-promise";
export const GS_FIRST_LINE_ID = "gs-first-line";

export const STEP_ORDER: GettingStartedStepId[] = [
	"connect_ai",
	"introductions",
	"connect_world",
	"interview",
];

export function gsCardId(step: GettingStartedStepId): string {
	return `${GS_PREFIX}card-${step}`;
}

export function gsCardStep(id: string): GettingStartedStepId | null {
	const m = id.match(/^gs-card-(connect_ai|introductions|connect_world|interview)$/);
	return (m?.[1] as GettingStartedStepId | undefined) ?? null;
}

export function isGettingStartedChat(convId: string | null | undefined): boolean {
	return convId === GETTING_STARTED_CHAT_ID;
}

/** The slash command that does what the door does. Typed, not discovered:
 *  the one place "onboarding" survives in the visible vocabulary. */
export const SKIP_COMMAND = "/dangerously-skip-onboarding";

/** The authored first line: shown the moment AI is connected and the room
 *  has no stored messages. No model call — the model speaks on the reply. */
export const FIRST_LINE =
	"Your server can think now. I’m here for the rest of the setup: " +
	"say what you’d like to be called, ask why any of this matters, or " +
	"just work through the cards above. Nothing here expires.";

/** Rebuild the synthetic top of the room from the derived state.
 *
 *  PREPENDS after stripping: the load paths replace `chat.messages`
 *  wholesale, and the state changes underneath (a source lands, the
 *  interview closes in its own room), so this must be safe to run any
 *  number of times. `state === null` (not loaded, or an older box without
 *  the endpoint) leaves the stored transcript alone. */
export function applyGettingStartedOpening(
	chat: Chat,
	convId: string | null | undefined,
	state: GettingStartedState | null,
): void {
	if (!isGettingStartedChat(convId)) return;
	const stored = chat.messages.filter((m) => !m.id.startsWith(GS_PREFIX));
	if (!state) {
		if (stored.length !== chat.messages.length) {
			chat.messages = stored as typeof chat.messages;
		}
		return;
	}
	// A single space, not empty: an assistant message with no text reads as
	// "loading" to the room's chrome. The card branch never renders it.
	const blank = { type: "text", text: " " } as const;
	const synthetic: { id: string; role: "assistant"; parts: (typeof blank)[] }[] = [];
	synthetic.push({ id: GS_MAST_ID, role: "assistant", parts: [blank] });
	for (const step of STEP_ORDER) {
		const s = state.steps.find((x) => x.id === step);
		if (s?.status === "open") synthetic.push({ id: gsCardId(step), role: "assistant", parts: [blank] });
	}
	const world = state.steps.find((s) => s.id === "connect_world");
	if (world?.status === "done" || state.first_day) {
		synthetic.push({ id: GS_PROMISE_ID, role: "assistant", parts: [blank] });
	}
	if (state.ai_connected && stored.length === 0) {
		synthetic.push({ id: GS_FIRST_LINE_ID, role: "assistant", parts: [blank] });
	}
	// Reassign ONLY when the synthetic set actually changed. The store's poll
	// hands out a fresh state object every 30s, and reassigning the transcript
	// on each one replaced the live message under the SDK's feet mid-stream:
	// a streamed tool part landed in the SDK's own object and was gone from
	// the copy the room rendered (the introductions card never appeared until
	// a reload).
	const currentSynthetic = chat.messages
		.filter((m) => m.id.startsWith(GS_PREFIX))
		.map((m) => m.id)
		.join("|");
	if (currentSynthetic === synthetic.map((m) => m.id).join("|")) return;
	chat.messages = [...synthetic, ...stored] as unknown as typeof chat.messages;
}
