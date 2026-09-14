/**
 * Getting started — the room after the founder's letter.
 *
 * One fixed conversation, seeded at boot; the server forces its mode by id
 * (see chat_handler) and refuses to delete or retitle it. It reads as ONE
 * thread: the assistant says what has been settled and asks the next thing
 * at the bottom, the person answers in the one composer, and a button
 * appears only where a button is genuinely needed (a subscription, a source,
 * the interview's door).
 *
 * Everything not typed by the person or answered by the model is SYNTHETIC
 * and never persisted: a welcome line and the settled steps at the top, the
 * current step's ask at the bottom. Rebuilt from the derived state on every
 * load and every state change, so someone back after a week sees today.
 * The interview room does the same with its opening
 * (chat/interview/interview.ts); this module is that pattern's second use.
 */
import type { Chat } from "@ai-sdk/svelte";
import type { GettingStartedState, GettingStartedStep, GettingStartedStepId } from "$lib/api/client";

/** Mirrors getting_started::GETTING_STARTED_CHAT_ID on the server. */
export const GETTING_STARTED_CHAT_ID = "chat_getting_started";

/** Every synthetic message id starts with this. */
export const GS_PREFIX = "gs-";
export const GS_WELCOME_ID = "gs-welcome";
/** The bottom of the thread: the current ask, the promise, the close. */
export const GS_NOW_PREFIX = "gs-now-";
export const GS_PROMISE_ID = "gs-now-promise";
export const GS_SETTLED_ID = "gs-now-settled";

export const STEP_ORDER: GettingStartedStepId[] = [
	"connect_ai",
	"introductions",
	"connect_world",
	"interview",
];

export function isGettingStartedChat(convId: string | null | undefined): boolean {
	return convId === GETTING_STARTED_CHAT_ID;
}

/** The step a bottom ask belongs to, if the id is one. */
export function gsNowStep(id: string): GettingStartedStepId | null {
	const m = id.match(/^gs-now-(connect_ai|introductions|connect_world|interview)$/);
	return (m?.[1] as GettingStartedStepId | undefined) ?? null;
}

/** The slash command that does what the door does. Typed, not discovered:
 *  the one place "onboarding" survives in the visible vocabulary. */
export const SKIP_COMMAND = "/dangerously-skip-onboarding";

// ── the authored lines ───────────────────────────────────────────────────
// The room's own voice: plain, no flattery, one thing at a time. The model
// speaks only in stored turns; these are the box's.

/** The heading: what this is and why, in two lines, and where you are in
 *  it. The one place the room says "step N of 4" — a sentence, not a list. */
function heading(stepIndex: number | null): string {
	const where =
		stepIndex === null
			? "All four done."
			: `Step ${stepIndex + 1} of 4 · about ten minutes in all`;
	return (
		"# Getting started\n\n" +
		"Your server keeps the record of your life, but it cannot connect itself, does not know your name, and cannot tell your story. " +
		"Four things, done here, one at a time. Nothing expires; leave and come back any time.\n\n" +
		where
	);
}

/** What a settled step reads as, in the thread's history. */
function settledLine(s: GettingStartedStep): string {
	if (s.status === "skipped") {
		switch (s.id) {
			case "introductions":
				return "Introductions, skipped for now. Say the word to come back to it.";
			case "connect_world":
				return "Connecting your world, skipped for now. Sources stay in Settings.";
			case "interview":
				return "The story of your life, skipped for now. The interview waits under Chats whenever you want it.";
			default:
				return "Connecting AI, skipped for now. Your server shows its record but cannot answer until AI is connected in Settings.";
		}
	}
	switch (s.id) {
		case "connect_ai":
			return s.via === "byo"
				? "AI is connected, through an endpoint of your own."
				: "AI is connected, through your Virtues subscription.";
		case "introductions":
			return "Introductions are recorded.";
		case "connect_world":
			return s.detail
				? `Your world is connected, with one thing to see to: ${s.detail}.`
				: "Your world is connected. The record has begun.";
		case "interview":
			return "Your story is written up, in your own words.";
	}
}

/** The ask for the step that is up now. */
function askLine(s: GettingStartedStep, first: boolean): string {
	const lead = first ? "First" : "Next";
	switch (s.id) {
		case "connect_ai":
			return `${lead}, give your server a mind. A Virtues subscription covers the models, web search, maps, and bank links, metered per request and never kept; an endpoint of your own covers the models alone. Until one is connected this room cannot answer, and nothing typed here goes anywhere.`;
		case "introductions":
			return `${lead}, introductions. What should I call you, what will you call me, where is home, and when were you born? Say it below in your own words, all at once is fine. The story of your life comes later, in its own conversation.`;
		case "connect_world":
			return `${lead}, your world: your accounts, this computer, your phone. Your server reads them from here on, and writes up your first day overnight from what they hold.`;
		case "interview":
			return s.underway
				? "Last, your story. The interview is underway; pick it up where you left it, one question at a time."
				: "Last, your story. A conversation about your life, about twenty minutes, one question at a time. Stop anywhere; it keeps your place.";
	}
}

function promiseLine(firstDay: string | null): string {
	return firstDay
		? "Your first day is written up."
		: "Your first day is written overnight from what your sources hold, and every day after writes itself.";
}

const SETTLED =
	"All four are settled. This room stays for questions about the setup; the rest of the app is yours.";

/** The text a synthetic message carries (ChatView renders it as markdown). */
function textMessage(id: string, text: string) {
	return { id, role: "assistant" as const, parts: [{ type: "text" as const, text }] };
}

/** Rebuild the synthetic top and bottom of the room from the derived state.
 *
 *  Top: the welcome, then one line per settled step, in walking order.
 *  Bottom: the ask for the first open step (and only that one), the promise
 *  once something is flowing, or the close once everything is settled.
 *  Stored turns sit between. Safe to run any number of times; rebuilds only
 *  when the synthetic set actually changed, and never while a turn streams
 *  (the caller checks status) — reassigning the transcript under the SDK
 *  mid-stream once lost a streamed tool part. `state === null` (not loaded,
 *  or an older box without the endpoint) leaves the stored transcript alone. */
export function applyGettingStartedOpening(
	chat: Chat,
	convId: string | null | undefined,
	state: GettingStartedState | null,
): void {
	if (!isGettingStartedChat(convId)) return;
	const stored = chat.messages.filter((m) => !m.id.startsWith(GS_PREFIX));
	if (!state) {
		if (stored.length !== chat.messages.length) chat.messages = stored as typeof chat.messages;
		return;
	}

	const steps = STEP_ORDER.map((id) => state.steps.find((s) => s.id === id)).filter(
		(s): s is GettingStartedStep => !!s,
	);
	const nowIndex = steps.findIndex((s) => s.status === "open");
	const now = nowIndex >= 0 ? steps[nowIndex] : undefined;

	// The history follows the walk, not the rows: a step that happens to be
	// done AHEAD of the one being asked (sources connected before AI on a
	// dev checkout, say) is not narrated yet, or the room would say "your
	// world is connected" before it has asked for anything.
	const top = [textMessage(GS_WELCOME_ID, heading(now ? nowIndex : null))];
	steps.forEach((s, i) => {
		if (s.status !== "open" && (nowIndex < 0 || i < nowIndex)) {
			top.push(textMessage(`gs-done-${s.id}`, settledLine(s)));
		}
	});

	const bottom: ReturnType<typeof textMessage>[] = [];
	const world = steps.find((s) => s.id === "connect_world");
	// The first day is written by a model, so the promise waits for one.
	if (state.ai_connected && (world?.status === "done" || state.first_day) && (nowIndex < 0 || nowIndex > 2)) {
		bottom.push(textMessage(GS_PROMISE_ID, promiseLine(state.first_day)));
	}
	if (now) {
		bottom.push(textMessage(`${GS_NOW_PREFIX}${now.id}`, askLine(now, now.id === "connect_ai")));
	} else if (state.graduated) {
		bottom.push(textMessage(GS_SETTLED_ID, SETTLED));
	}

	const want = [...top, ...bottom].map((m) => m.id).join("|");
	const have = chat.messages
		.filter((m) => m.id.startsWith(GS_PREFIX))
		.map((m) => m.id)
		.join("|");
	if (want === have) return;
	chat.messages = [...top, ...stored, ...bottom] as unknown as typeof chat.messages;
}

/** Before the person's turn goes out: the bottom asks come off, so the
 *  reply lands under their message and the ask returns beneath it after. */
export function stripGettingStartedBottom(chat: Chat, convId: string | null | undefined): void {
	if (!isGettingStartedChat(convId)) return;
	const kept = chat.messages.filter((m) => !m.id.startsWith(GS_NOW_PREFIX));
	if (kept.length !== chat.messages.length) chat.messages = kept as typeof chat.messages;
}
