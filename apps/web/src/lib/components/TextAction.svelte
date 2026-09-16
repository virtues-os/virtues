<script lang="ts">
	import type { Snippet } from "svelte";

	/**
	 * A text action: a verb you can press, with no box around it.
	 *
	 * design-grammar.md §6 has always had the rule — "Everything else is a
	 * quiet link in `--color-primary` at 14px/500 sans" — and on 2026-09-16
	 * nothing in the tree obeyed it. A sweep found **18 independent
	 * definitions** of this one affordance (seven of them literally named
	 * `.linkish`, in seven files, disagreeing with each other), spanning:
	 *
	 *   7 color tokens   --color-accent · --color-foreground · -subtle ·
	 *                    -muted · --color-primary · --color-error · inherit
	 *   7 font sizes     inherit · 0.75rem · 0.8125rem · 13px · 14px · 15px · 1rem
	 *   4 underlines     2px offset · 0.15em with a --color-border rule ·
	 *                    dotted at 3px · none at all
	 *
	 * One of those seven colors does not exist. `--color-accent` is written by
	 * `wiki/EntityArticleSection.svelte` and `wiki/NotesRail.svelte`, and
	 * `themes.css` never defines `--accent`, so the Tailwind `@theme inline`
	 * bridge never emits it — both sites have been silently rendering their
	 * fallback (`currentColor`) rather than an accent, on every theme, since
	 * they were written. That is the gallery's standing warning about unread
	 * `--color-*` bridges, caught in the wild. This component uses
	 * `--color-primary`, which §6 names and which resolves.
	 *
	 * TWO BOOLEANS, NOT A `variant`. This was drafted as
	 * `variant: "quiet" | "loud"`, but the evidence has two INDEPENDENT axes
	 * and an enum would need all four of their combinations:
	 *
	 *   inline   11 of the ~23 call sites sit mid-sentence — inside a `<p>`,
	 *            a `<dd>`, a table cell — where §6's 14px/500 sans would
	 *            break the line it sits in. Those keep `font: inherit` and
	 *            take only the color and the underline. The other 12 stand on
	 *            their own line and get §6's rule exactly. An inline action is
	 *            also the ONE case §6 exempts from the 44pt touch floor: a
	 *            44pt box around a word in a paragraph reaches into the lines
	 *            above and below and swallows their taps.
	 *   quiet    the way back, the second thought — "Back", "Not now",
	 *            "Cancel". `Act.svelte` reached the same conclusion
	 *            independently and calls it `plain`; 6 of its 16 call sites
	 *            use it, four of them the same "Back".
	 *
	 * The underline is not decoration and is not optional: color alone must
	 * never be the only thing marking a control, and §5 allows a colored
	 * thing in the pane only when it means *now* or *pressable*. The underline
	 * is what proves *pressable*. It is drawn at 40% of the ink so it stays
	 * quiet, and comes to full strength on hover — `wiki/PlacePage.svelte` had
	 * the best version of this idea and it is generalized here.
	 *
	 * `loading` swaps the words rather than overlaying a spinner, which is the
	 * opposite of what `Button` does, for a reason that is specific to having
	 * no box: five call sites already write `{writing ? 'Writing…' : 'Write
	 * the article'}` by hand, and a spinner laid over a bare phrase has
	 * nothing to sit on. The cost is the one `Button` avoids — the text
	 * changes width, and inline that reflows the sentence — so `loadingLabel`
	 * is opt-in, and an inline action that leaves it unset keeps its own words
	 * and just goes busy.
	 *
	 * `href` exists because `chat/ChatError.svelte` puts the identical class
	 * on two `<a>` elements and two `<button>` elements inside one block. A
	 * primitive that cannot be a link would regress that file the day it
	 * converts.
	 *
	 * There is deliberately no `icon` prop. Exactly one of the ~23 sites has
	 * one (a trailing arrow), §6 asks for verbs that name the place rather
	 * than arrows, and an icon inside an inline action breaks the line box it
	 * sits in. One site is not a primitive.
	 *
	 * KNOWN LIMIT, measured rather than assumed: `inline` asks for
	 * `display: inline`, and a `<button>` does not get it. Chrome's UA
	 * stylesheet forces buttons to `inline-block` and an author declaration
	 * does not override it (verified in the gallery: computed `inline-block`,
	 * and a long inline label reports ONE client rect — its text wraps inside
	 * the box, but the box itself never breaks across a line). So an inline
	 * action is an atomic unit: it sits on one line or moves to the next
	 * whole. That is invisible on the short verbs this mostly replaces
	 * ("Rename", "Edit", "Remove") and shows as a ragged gap on a long one
	 * ("What do you remember of 2019?"). An `href` action has the same limit
	 * whenever it is a flex item, since flex blockifies its children.
	 * There is no CSS fix; keep inline labels short.
	 */
	let {
		quiet = false,
		inline = false,
		loading = false,
		loadingLabel,
		disabled = false,
		href,
		type = "button",
		onclick,
		class: className = "",
		children,
	}: {
		/** The way back, the second thought. Muted ink instead of the accent. */
		quiet?: boolean;
		/** Sits mid-sentence: inherits the paragraph's face and size, and takes no touch halo. */
		inline?: boolean;
		/** Working. Stops taking clicks and says `aria-busy`. */
		loading?: boolean;
		/** What to say while `loading` — "Writing…", "Saving…". Omit to keep the label. */
		loadingLabel?: string;
		disabled?: boolean;
		/** Renders an `<a>` instead of a `<button>`. For real navigation only. */
		href?: string;
		type?: "button" | "submit" | "reset";
		onclick?: (e: MouseEvent) => void;
		class?: string;
		children: Snippet;
	} = $props();

	// A disabled link is not a thing the platform has — `<a>` ignores
	// `disabled` and stays clickable — so anything inert renders as a button.
	let asLink = $derived(href !== undefined && !disabled && !loading);
</script>

{#if asLink}
	<a
		{href}
		class="v-textaction {className}"
		data-quiet={quiet ? "true" : undefined}
		data-inline={inline ? "true" : undefined}
		{onclick}
	>
		{@render children()}
	</a>
{:else}
	<button
		{type}
		class="v-textaction {className}"
		data-quiet={quiet ? "true" : undefined}
		data-inline={inline ? "true" : undefined}
		disabled={disabled || loading}
		aria-busy={loading || undefined}
		{onclick}
	>
		{#if loading && loadingLabel}
			{loadingLabel}
		{:else}
			{@render children()}
		{/if}
	</button>
{/if}

<style>
	.v-textaction {
		display: inline;
		margin: 0;
		padding: 0;
		border: none;
		background: none;
		/* §6: a quiet link in --color-primary at 14px/500 sans. */
		font-family: var(--font-sans);
		font-size: 14px;
		font-weight: 500;
		color: var(--color-primary);
		text-align: inherit;
		cursor: pointer;
		/* Not decoration — the thing that marks this as pressable without
		   relying on color alone. Quiet at 40% of the ink, full on hover. */
		text-decoration: underline;
		text-underline-offset: 2px;
		text-decoration-thickness: from-font;
		text-decoration-color: color-mix(in srgb, currentColor 40%, transparent);
		-webkit-tap-highlight-color: transparent;
		transition:
			color 120ms ease,
			text-decoration-color 120ms ease;
	}

	.v-textaction:hover:not(:disabled) {
		text-decoration-color: currentColor;
	}

	.v-textaction:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	/* Busy is not disabled — the same distinction Button.svelte draws. A link
	   that is DOING the thing should not look like one that refuses to. */
	.v-textaction[aria-busy="true"]:disabled {
		opacity: 1;
		cursor: progress;
	}

	/* Keyboard only. A mouse press should not leave a ring behind it. */
	.v-textaction:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
		border-radius: 6px;
	}

	/* The way back. Muted ink, so "Cancel" beside "Send a link" reads as the
	   lesser of the two without either growing a box. */
	.v-textaction[data-quiet="true"] {
		color: var(--color-foreground-muted);
	}

	.v-textaction[data-quiet="true"]:hover:not(:disabled) {
		color: var(--color-foreground);
	}

	/* Mid-sentence. Takes the paragraph's face, size and weight — including
	   the serif, in the wiki's prose — and keeps only the color and the rule
	   under it. Setting 14px sans here instead is what would make a word in a
	   JJannon paragraph jump out of its own line. */
	.v-textaction[data-inline="true"] {
		font: inherit;
	}

	/* §6: the touch floor moves INTO the control. A standalone text action is
	   a line of its own, so it can afford the height; an inline one is the
	   section's named exception, because a 44pt box around a word reaches into
	   the lines above and below and swallows their taps. */
	@media (pointer: coarse) {
		.v-textaction:not([data-inline="true"]) {
			display: inline-flex;
			align-items: center;
			min-height: 44px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.v-textaction {
			transition: none;
		}
	}
</style>
