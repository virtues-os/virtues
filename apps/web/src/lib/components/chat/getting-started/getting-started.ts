/**
 * Getting started — the room after the founder's letter.
 *
 * One fixed conversation, seeded at boot; the server forces its mode by id
 * (see chat_handler) and refuses to delete or retitle it. THE ROOM'S OWN
 * LINES ARE REAL TURNS: the server appends each one once, in order, marked
 * by `subject` (see getting_started.rs `narrate`), so the client places
 * nothing and the thread cannot repeat itself. Until 2026-09-14 the client
 * re-rendered those lines from state on every change, which asked a
 * question again underneath the answer to it.
 *
 * What stays here: the room's id, the slash command, and the interview's
 * opening — which is the INTERVIEW's, not the room's, so it is shown
 * without persisting (one copy of that text, in chat/interview) and placed
 * among the turns by the instant the interview began.
 */
import type { Chat } from "@ai-sdk/svelte";
import type { GettingStartedState } from "$lib/api/client";
import {
	INTERVIEW_OPENING_LEAD,
	INTERVIEW_OPENING_BODY,
	INTERVIEW_OPENING_ASK,
} from "$lib/components/chat/interview/interview";

/** Mirrors getting_started::GETTING_STARTED_CHAT_ID on the server. */
export const GETTING_STARTED_CHAT_ID = "chat_getting_started";

/** The interview's opening, shown in the room, never persisted. */
export const GS_INTERVIEW_PREFIX = "gs-iv-";
export const GS_INTERVIEW_OPENING_ID = "gs-iv-opening";

/** The slash command that does what the door does. Typed, not discovered:
 *  the one place "onboarding" survives in the visible vocabulary. */
export const SKIP_COMMAND = "/dangerously-skip-onboarding";

export function isGettingStartedChat(convId: string | null | undefined): boolean {
	return convId === GETTING_STARTED_CHAT_ID;
}

function opening(id: string, text: string) {
	return { id, role: "assistant" as const, parts: [{ type: "text" as const, text }] };
}

/**
 * Put the interview's opening in the thread at the instant the interview
 * began: after the setup turns, before the interview's own. Stored turns
 * carry `createdAt`; a turn still streaming has none and is after the
 * boundary by construction. Safe to run repeatedly, and it does nothing
 * until the interview has started.
 */
export function applyInterviewOpening(
	chat: Chat,
	convId: string | null | undefined,
	state: GettingStartedState | null,
): void {
	if (!isGettingStartedChat(convId)) return;
	const stored = chat.messages.filter((m) => !m.id.startsWith(GS_INTERVIEW_PREFIX));
	const startedAt = state?.interview_started_at;
	if (!startedAt) {
		if (stored.length !== chat.messages.length) chat.messages = stored as typeof chat.messages;
		return;
	}
	const already = chat.messages.some((m) => m.id === GS_INTERVIEW_OPENING_ID);
	const boundary = new Date(startedAt).getTime();
	const before: typeof stored = [];
	const after: typeof stored = [];
	for (const m of stored) {
		const at = (m as { createdAt?: Date | string }).createdAt;
		const t = at ? new Date(at).getTime() : Number.POSITIVE_INFINITY;
		(t >= boundary ? after : before).push(m);
	}
	const lines = [
		// The heading and its lead share one message: the plate renders
		// after that message's text, so the lead stands between them.
		opening(GS_INTERVIEW_OPENING_ID, "## The story of your life: chapters & identity\n\n" + INTERVIEW_OPENING_LEAD),
		opening("gs-iv-body", INTERVIEW_OPENING_BODY),
		opening("gs-iv-ask", INTERVIEW_OPENING_ASK),
	];
	// Nothing to do when the opening already sits where it belongs.
	if (already && chat.messages.length === stored.length + lines.length) {
		const at = chat.messages.findIndex((m) => m.id === GS_INTERVIEW_OPENING_ID);
		if (at === before.length) return;
	}
	chat.messages = [...before, ...lines, ...after] as unknown as typeof chat.messages;
}
