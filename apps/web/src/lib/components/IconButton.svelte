<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	/**
	 * The one icon-only button — a square control that carries a glyph and no
	 * words: a close, a toggle, a step, a `···`.
	 *
	 * It exists because `Button` deliberately refuses this shape. `Button`
	 * keeps `children` required so an accessible name can never go missing,
	 * and that is the right call there — but it left 59 icon-only call sites
	 * with nowhere to go, and all 59 hand-rolled their own. A triage on
	 * 2026-09-16 found ONE control drawn seventeen ways: ten box sizes (16,
	 * 18, 20, 24, 26, 28, 30, 32, 36, 44) and six radii (4, 6, 8, 10, 999px,
	 * `--radius-full`). Nine of those ten rows violate design-grammar.md §6,
	 * which allows `{0, 6px, 12px, 50%, a pill}` and nothing else.
	 *
	 * None of that variance was a decision. It is what happens when a shape is
	 * copied rather than named — the same sentence §6 already writes about the
	 * 22 hand-rolled card blocks.
	 *
	 * WHAT COLLAPSED, and to what:
	 *
	 *   size    the four real clusters, not ten sizes. 16/18/20 → `xs`;
	 *           24/26 → `sm`; 28/30/32 → `md`; 36/44 → `touch`.
	 *   radius  6px at every size. Six radii, one answer — the shell's radius
	 *           (design.md), and already the plurality of what shipped.
	 *   ink     `--color-foreground-muted`, the same ink `Button variant="ghost"`
	 *           uses. The tree was split between `-muted` and `-subtle` with no
	 *           rule behind the split.
	 *   ground  `--hover-bg` / `--active-bg`, the foreground-mix ramp. Four of
	 *           the hand-rolled hovers reached for `--color-background-hover`
	 *           or `--color-surface-elevated` instead, which BOTH bridge to
	 *           `--surface-elevated` — a 3-4% step that reads as nothing
	 *           happening. design.md names that exact bug as having shipped
	 *           twice; do not reintroduce it.
	 *
	 * `variant` borrows `Button`'s vocabulary on purpose, rather than the
	 * `ghost | quiet | danger` this was drafted with. Two primitives that name
	 * the same job differently is the disease this whole pass exists to end:
	 * a bordered control on a surface is `secondary` in `Button`, so it is
	 * `secondary` here, and there is nothing left for "quiet" to mean that
	 * `ghost` does not already say.
	 *
	 *   ghost      a control in chrome; nothing on the desk may out-shout the
	 *              page (design.md). 15 of the 19 surveyed sites.
	 *   secondary  a real control that needs an edge to be findable — a
	 *              toolbar affordance sitting on the page rather than in the
	 *              chrome. 2 sites.
	 *   danger     destroys something.
	 *
	 * `label` is required and feeds BOTH `aria-label` and `title`, for the
	 * reason `Button`'s `children` is required: an icon with no accessible
	 * name is a button that announces itself as "button", and the only way to
	 * make that impossible to get wrong is to leave no way to express it. A
	 * `<title>` on the icon would not do it — the glyph is `aria-hidden`.
	 *
	 * `pressed` is a toggle that is ON, and it is drawn in `--color-primary`
	 * because grammar §5 gives the accent exactly two meanings — *now* and
	 * *pressable* — and a lit toggle is *now*. Four of the five hand-rolled
	 * toggle states had already arrived at the accent independently.
	 */
	let {
		icon,
		label,
		size = "md",
		variant = "ghost",
		pressed,
		expanded,
		haspopup,
		disabled = false,
		type = "button",
		onclick,
		class: className = "",
	}: {
		/** An iconify name, e.g. `ri:close-line`. */
		icon: string;
		/**
		 * The accessible name — what the button DOES, as a verb where there is
		 * one ("Close", "Add filter"). Becomes `aria-label` and `title`.
		 */
		label: string;
		/** ~20 / 24 / 28 / 44px. `touch` is a painted size, not a hit area — see the CSS. */
		size?: "xs" | "sm" | "md" | "touch";
		variant?: "ghost" | "secondary" | "danger";
		/** A toggle's state. Renders `aria-pressed`; omit entirely for a plain button. */
		pressed?: boolean;
		/** For a disclosure or popover trigger: renders `aria-expanded`. */
		expanded?: boolean;
		/** For a popover trigger: renders `aria-haspopup`. */
		haspopup?: "menu" | "listbox" | "tree" | "grid" | "dialog" | true;
		disabled?: boolean;
		type?: "button" | "submit" | "reset";
		onclick?: (e: MouseEvent) => void;
		class?: string;
	} = $props();
</script>

<button
	{type}
	class="v-iconbtn {className}"
	data-size={size}
	data-variant={variant}
	{disabled}
	title={label}
	aria-label={label}
	aria-pressed={pressed}
	aria-expanded={expanded}
	aria-haspopup={haspopup}
	{onclick}
>
	<!-- The glyph is decoration: `label` above is the whole accessible name, so
	     letting the icon contribute one would double it. -->
	<Icon {icon} width="1em" height="1em" aria-hidden="true" />
</button>

<style>
	.v-iconbtn {
		position: relative;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		padding: 0;
		border: 1px solid transparent;
		/* One radius at every size. The shell's 6px (design.md), which grammar
		   §6 allows and which was already the plurality of the seventeen. */
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-family: var(--font-sans);
		cursor: pointer;
		/* We paint our own :active ground; the platform's gray flash on touch
		   would land a beat later and in a color no theme chose. */
		-webkit-tap-highlight-color: transparent;
		transition:
			background-color 120ms ease,
			border-color 120ms ease,
			color 120ms ease;
	}

	/* An inline SVG sits on the text baseline and hangs a descender's worth of
	   space below it; `block` takes it off the baseline so the flex box centers
	   the glyph on the square instead of on its own line box. (Button.svelte
	   carries the same note, for the same reason.) */
	.v-iconbtn :global(svg) {
		display: block;
	}

	.v-iconbtn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	/* Keyboard only. A mouse press should not leave a ring behind it. */
	.v-iconbtn:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
	}

	/* Sizes — the four clusters the tree actually had. Every value is a
	   multiple of 4; 28px is the shell's own tab height and 44px is the touch
	   floor, so neither is invented here. */
	.v-iconbtn[data-size="xs"] {
		width: 20px;
		height: 20px;
		font-size: 14px;
	}

	.v-iconbtn[data-size="sm"] {
		width: 24px;
		height: 24px;
		font-size: 16px;
	}

	.v-iconbtn[data-size="md"] {
		width: 28px;
		height: 28px;
		font-size: 16px;
	}

	.v-iconbtn[data-size="touch"] {
		width: 44px;
		height: 44px;
		font-size: 20px;
	}

	/* THE TOUCH FLOOR. Grammar §6: "Touch targets move INTO the control, not
	   around it" — the 44pt floor is cleared by the thing you press, not by
	   the space beside it. §6 gets there by having the row zero its padding
	   and the button take it, which needs the row's cooperation and therefore
	   cannot live in a primitive.

	   So the hit area grows without the painted box growing: a transparent
	   pseudo-element centered on the control, at least 44px square. Layout is
	   untouched, so a 20px tab-close stays 20px in its dense row and nothing
	   reflows — but on a phone or tablet it is a 44pt target rather than a
	   20pt one. That is why `touch` is a size and ALSO why it is not the
	   whole answer: `touch` is for chrome that should be VISIBLY 44px (the
	   mobile bar, the drawer's close), and this rule is for every control
	   that merely has to be pressable by a thumb.

	   The caveat, so it is not rediscovered: two of these closer together than
	   44px have overlapping halos, and the later one in the DOM takes the
	   overlap. A row of icon buttons meant for a phone wants `touch` and real
	   spacing, not this. */
	@media (pointer: coarse) {
		.v-iconbtn::after {
			content: "";
			position: absolute;
			top: 50%;
			left: 50%;
			width: max(100%, 44px);
			height: max(100%, 44px);
			transform: translate(-50%, -50%);
		}
	}

	/* Chrome. design.md: nothing on the desk may out-shout the page, so this
	   carries no ground and no edge until you reach for it. */
	.v-iconbtn[data-variant="ghost"]:hover:not(:disabled) {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.v-iconbtn[data-variant="ghost"]:active:not(:disabled) {
		background: var(--active-bg);
	}

	.v-iconbtn[data-variant="secondary"] {
		background: var(--color-surface);
		border-color: var(--color-border);
		color: var(--color-foreground-muted);
	}

	.v-iconbtn[data-variant="secondary"]:hover:not(:disabled) {
		background: var(--hover-bg);
		border-color: var(--color-border-strong);
		color: var(--color-foreground);
	}

	.v-iconbtn[data-variant="secondary"]:active:not(:disabled) {
		background: var(--active-bg);
	}

	/* Destroys something. Quiet until touched — an always-red icon in a
	   toolbar states the severity before anything is at stake, and §5 asks a
	   condition to be said once, where it can be acted on. */
	.v-iconbtn[data-variant="danger"]:hover:not(:disabled) {
		background: var(--hover-bg);
		color: var(--color-error);
	}

	.v-iconbtn[data-variant="danger"]:active:not(:disabled) {
		background: var(--active-bg);
	}

	/* A toggle that is ON. The accent means *now* (§5), and it outranks the
	   variant's own ink — a pressed danger toggle is still "on". */
	.v-iconbtn[aria-pressed="true"]:not(:disabled) {
		background: var(--active-bg);
		color: var(--color-primary);
	}

	.v-iconbtn[aria-pressed="true"]:hover:not(:disabled) {
		color: var(--color-primary);
	}

	@media (prefers-reduced-motion: reduce) {
		.v-iconbtn {
			transition: none;
		}
	}
</style>
