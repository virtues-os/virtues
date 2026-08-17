/**
 * The six steps of onboarding, in order.
 *
 * Here rather than inside OnboardingHeader.svelte because the page needs the
 * type to seed and advance `screen`, and a type exported from a component's
 * instance script is not importable — it would have to live in a `<script
 * module>` block, which is a lot of ceremony for a list.
 *
 * Labels are what the person would call them, not what the API calls them.
 */

export type StepId = "letter" | "names" | "account" | "sources" | "words" | "you";

export interface Step {
	id: StepId;
	label: string;
	icon: string;
}

export const STEPS: Step[] = [
	{ id: "letter", label: "The letter", icon: "ri:quill-pen-line" },
	{ id: "names", label: "Introductions", icon: "ri:user-3-line" },
	{ id: "account", label: "Account", icon: "ri:shield-keyhole-line" },
	{ id: "sources", label: "Your world", icon: "ri:links-line" },
	{ id: "words", label: "In your own words", icon: "ri:chat-quote-line" },
	{ id: "you", label: "You", icon: "ri:sparkling-2-line" },
];
