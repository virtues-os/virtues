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
	void stepIndex;
	return (
		"# Getting started\n\n" +
		"Virtues keeps the record of your life on this server and writes it up for you, one day at a time. " +
		"Before it can begin, it needs four things from you that it has no way of discovering on its own: something to think with, your name, something to read, and the story of your life so far. " +
		"This conversation walks through them in order. The first three take a few minutes; the last is a longer conversation, for whenever you are ready. You can leave at any point and pick up where you stopped."
	);
}

/** What a settled step reads as, in the thread's history. */
function settledLine(s: GettingStartedStep): string {
	if (s.status === "skipped") {
		switch (s.id) {
			case "introductions":
				return "You set introductions aside for now. Say the word whenever you would like to return to them.";
			case "connect_world":
				return "You set the record aside for now. Your sources are waiting in Settings whenever you want them.";
			case "interview":
				return "You set your story aside for now. The interview is waiting under Chats whenever you want it.";
			default:
				return "You went on without connecting AI. Your server can show you its record, but it cannot answer you until a model is connected in Settings.";
		}
	}
	switch (s.id) {
		case "connect_ai":
			return s.via === "byo"
				? "Your server is connected to an endpoint of your own, and can think."
				: "Your server is connected to your Virtues subscription, and can think.";
		case "introductions":
			return "Introductions are made.";
		case "connect_world":
			return s.detail
				? `The record has begun, with one thing still to see to: ${s.detail}.`
				: "The record has begun.";
		case "interview":
			return "Your story is written down, in your own words.";
	}
}

/** The ask for the step that is up now. */
function askLine(s: GettingStartedStep): string {
	switch (s.id) {
		case "connect_ai":
			return "Everything Virtues does begins with a model to think with. A Virtues subscription gives you the best of Claude, Gemini, GPT and Grok, all of them under zero data retention, which means each request is metered and nothing you send is kept or trained on. If you already have an account, sign in. If you run models of your own, you can point your server at them instead. Until one of these is in place, this room cannot answer you.";
		case "introductions":
			return "Now that it can think, your server would like to know who it is talking to. Tell it what you like to be called, what you will call it, where home is, and when you were born, all in one message if you like. The birthday is not idle curiosity; it is the ruler your whole life is drawn against. The longer story of your life comes later, in a conversation of its own.";
		case "connect_world":
			return "The record is written from what your accounts, this computer, and your phone already hold, and nothing they hold ever leaves your server. Connect one of them now, and tonight the first page will be written.";
		case "interview":
			return s.underway
				? "Last comes your story, and the interview is already underway. It has kept your place; pick it up wherever you like."
				: "Last comes your story. The record can hold what happened, but only you can say what it meant. This is a conversation of about twenty minutes, one question at a time; stop wherever you like, and it will keep your place.";
	}
}

function promiseLine(firstDay: string | null): string {
	return firstDay
		? "Your first page is written: yesterday, written down."
		: "Every day, a page will be waiting for you: yesterday, written down. The first one comes tomorrow morning.";
}

const SETTLED = "That is all four. This room stays open for any question about the setup, and the rest of Virtues is yours.";

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
		bottom.push(textMessage(`${GS_NOW_PREFIX}${now.id}`, askLine(now)));
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
