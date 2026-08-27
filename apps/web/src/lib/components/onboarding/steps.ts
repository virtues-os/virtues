/**
 * The four steps of onboarding, in order.
 *
 * Here rather than inside OnboardingHeader.svelte because the page needs the
 * type to seed and advance `screen`, and a type exported from a component's
 * instance script is not importable — it would have to live in a `<script
 * module>` block, which is a lot of ceremony for a list.
 *
 * Labels are what the person would call them, not what the API calls them.
 *
 * THERE IS NO ACCOUNT STEP. The account is a SETUP fact — the airlock's BLE
 * link step handles it, before pairing, and it is skippable there. Sources
 * need no account on either side (the OAuth proxy's exchange leg is
 * deliberately unauthenticated; chat import and the collector are local), so
 * the only thing on the box that needs it is an AI call. The gate therefore
 * renders as a conditional interstitial at the reveal, for exactly the people
 * who skipped linking — a toll booth, not a story beat (2026-08-21).
 *
 * THERE IS NO INTERVIEW STEP EITHER (2026-08-27). The narrative interview is
 * the product's first conversation — one chat in the real app (see
 * ChatView's INTERVIEW_CHAT_ID) — not an onboarding surface. Onboarding is
 * done when the record is flowing; the reveal's door points at the waiting
 * conversation. Three form factors died teaching us this (textareas, chapter
 * cards, a bespoke chat) — docs/lsi-plan.md has the history.
 */

export type StepId = "letter" | "names" | "sources" | "you";

export interface Step {
	id: StepId;
	label: string;
	icon: string;
}

export const STEPS: Step[] = [
	{ id: "letter", label: "Founder's letter", icon: "ri:quill-pen-line" },
	{ id: "names", label: "Introductions", icon: "ri:user-3-line" },
	{ id: "sources", label: "Your data", icon: "ri:links-line" },
	{ id: "you", label: "You", icon: "ri:sparkling-2-line" },
];

/**
 * The URL space — `/onboarding/<view>`.
 *
 * Slugs are written for a human reading the address bar, which is why they
 * are not simply the step ids.
 */
export type ViewId = "letter" | "introductions" | "sources" | "you";

/** Reading order. Comparing two positions here is what decides which way the page turns. */
export const VIEW_ORDER: ViewId[] = ["letter", "introductions", "sources", "you"];

export const VIEW_STEP: Record<ViewId, StepId> = {
	letter: "letter",
	introductions: "names",
	sources: "sources",
	you: "you",
};

/** The first view of a step, for the strip's backward jumps. */
export const STEP_VIEW: Record<StepId, ViewId> = {
	letter: "letter",
	names: "introductions",
	sources: "sources",
	you: "you",
};

export function isViewId(s: string | undefined | null): s is ViewId {
	return !!s && s in VIEW_STEP;
}
