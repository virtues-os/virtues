/**
 * The narrative interview — the one chat that is not a chat.
 *
 * One fixed conversation, seeded at boot; the server forces interview mode
 * by its id (see chat_handler) and refuses to retitle it. Everything the
 * chat room does differently for this one conversation keys on the constants
 * and helpers here, so ChatView carries the branches and not the substance:
 * the authored opening, the close detection, the id itself.
 */
import type { Chat } from "@ai-sdk/svelte";

/** Mirrors narrative_draft::INTERVIEW_CHAT_ID on the server. */
export const INTERVIEW_CHAT_ID = "chat_narrative_interview";

/** The synthetic first message's id — never persisted, never sent. The
 *  room keys the lifeline plate and the bleed on it. */
export const INTERVIEW_OPENING_ID = "interview-opening";

export function isInterviewChat(convId: string | null | undefined): boolean {
	return convId === INTERVIEW_CHAT_ID;
}

/** The opening, in three parts: the heading alone (the lifeline plate
 *  renders right after it, bleeding past the column — see the message
 *  template in ChatView), then the intro and the example table, then the ask. */
export const INTERVIEW_OPENING = "# The story of your life: chapters & identity";

/** Under the heading, before the plate: what the plate is an example of,
 *  and why the interview exists at all. The "why" is the point — the
 *  record holds what happened; this is how the AI is grounded in who the
 *  person is and what they value, not just in what they did. */
export const INTERVIEW_OPENING_LEAD =
	"This lifeline is an example of what you will make here: your life from " +
	"beginning to end, its chapters, its turning points, and the stories " +
	"that matter. It gives the AI a grounding in who you are, your " +
	"temperament, your virtues and vices, and the person you want to become.";

export const INTERVIEW_OPENING_BODY =
	"In order to help the Virtues platform generate more powerful insights " +
	"in your life, we’ll guide you in briefly describing your past " +
	"chapters.\n\n" +
	"We define chapters as seven major arcs in your life; see the table " +
	"below for an example.\n\n" +
	// A made-up life (see ChapterLifeline.svelte, which draws the same
	// one). The interview prompt tells the model this table is an
	// example, and the repo's rule is that nothing from a real life ships.
	"| Chapter | Years |\n" +
	"|---|---|\n" +
	"| Childhood on the coast | 1997 – 2003 |\n" +
	"| Grade school, inland | 2003 – 2009 |\n" +
	"| The band years | 2009 – 2016 |\n" +
	"| College | 2016 – 2021 |\n" +
	"| The first shop | 2021 – 2023 |\n" +
	"| The workshop | 2023 – 2025 |\n" +
	"| Out on my own | 2025 – now |";

/** The ask comes last, after the shape has been seen; the retention
 *  promise rides with it because it is the one thing to know before
 *  answering. */
export const INTERVIEW_OPENING_ASK =
	"Yours will look nothing like these. What would your chapters be? " +
	"Rough names and rough years are enough; months and dates are welcome " +
	"where you remember them.\n\n" +
	"What you say here stays on your server. The model conducting this is " +
	"sent your words under a no-retention agreement and keeps nothing.";

/** The narrative interview opens ALREADY SPEAKING: an authored first line,
 *  shown free (never persisted, no model call). The interview prompt knows
 *  this opening was delivered and picks up from the reply.
 *
 *  Called from BOTH of ChatView's load paths — the tab-change effect and
 *  onMount. It lived inline in the first one only, so switching to an open
 *  interview tab greeted you and deep-linking to /chat/chat_narrative_interview
 *  (a fresh page load, a restored tab, the Home link) opened a blank room
 *  with no explanation of what it was for.
 *
 *  PREPENDS rather than requiring an empty room: the opening is never
 *  persisted, so a reload mid-interview would otherwise start the
 *  transcript at the person's first reply with no trace of what was
 *  asked. The backend rebuilds model context from its own store, so the
 *  synthetic message rides the UI only. */
export function applyInterviewOpening(chat: Chat, convId: string | null | undefined): void {
	if (!isInterviewChat(convId)) return;
	if (chat.messages[0]?.id === INTERVIEW_OPENING_ID) return;
	chat.messages = [
		{
			id: INTERVIEW_OPENING_ID,
			role: "assistant",
			// Three parts on purpose: the lifeline plate renders after
			// the first (the heading), so the shape is seen before the
			// example table, and the ask lands last.
			parts: [
				{ type: "text", text: INTERVIEW_OPENING + "\n\n" + INTERVIEW_OPENING_LEAD },
				{ type: "text", text: INTERVIEW_OPENING_BODY },
				{ type: "text", text: INTERVIEW_OPENING_ASK },
			],
		},
		...chat.messages,
	] as unknown as typeof chat.messages;
}

/** What write_it_up reports when the close succeeds. */
export interface WriteItUpOutput {
	document_page_id: string;
	document_already_existed?: boolean;
	chapters_written?: number;
	chapters_error?: string;
}

/** The close, as witnessed by the transcript: the latest write_it_up part
 *  that produced a document. One of three witnesses ChatView consults —
 *  the others are the transient data part from this session and the box's
 *  setup state — any one of which retires the composer. */
export function findWriteItUpOutput(messages: readonly unknown[]): WriteItUpOutput | null {
	for (let i = messages.length - 1; i >= 0; i--) {
		const m = messages[i] as any;
		if (m.role !== "assistant") continue;
		for (const part of m.parts ?? []) {
			if (
				part.type === "tool-write_it_up" &&
				part.state === "output-available" &&
				part.output?.document_page_id
			) {
				return part.output as WriteItUpOutput;
			}
		}
	}
	return null;
}
