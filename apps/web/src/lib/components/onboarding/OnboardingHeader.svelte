<!--
  The one header every onboarding surface wears.

  WHY IT EXISTS. Onboarding is five separate surfaces — the letter, the two
  names, the document, the interview, the draft — and each one used to open with
  its own eyebrow and no way to tell where it sat in the whole. Someone four
  screens in had no idea whether they were near the end or near the start, which
  is the single most common reason people abandon a flow they had already paid
  for.

  A STRIP, NOT A STEPPER. Numbered stepper bars claim a rigidity this flow does
  not have: sources are optional, the interview can be left half-written, and
  the order is only mostly fixed. What this shows is position, not permission.

  NOTHING IN THE STRIP EVER RESIZES. Six icons of one fixed size, and the name
  of where you are lives in the eyebrow on the far left instead. Two earlier
  versions put that name inside the current pill: it re-laid out the row every
  time you advanced, which read as a flick. Left-anchored text next to a
  right-anchored strip can change length without moving anything.

  Pressing a mark goes BACK to a step already behind you; forward marks are
  disabled, because a strip that could skip the account gate would be lying
  about what onboarding requires.

  IT REPLACED THE LEFT RAIL. The document surface carried a table of contents
  listing these same chapters, so a global strip alongside it would have been
  two progress systems disagreeing at the edges. One of them had to go, and the
  rail could only ever appear on the one surface that was long enough to scroll.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { STEPS, type StepId } from "./steps";

	interface Props {
		/** Where the person is now. */
		step: StepId;
		/** Steps finished — VISUAL WEIGHT ONLY. Never gates navigation. */
		done?: StepId[];
		/**
		 * Steps this person may navigate to, decided by the route.
		 *
		 * Separate from `done` because the two answer different questions, and
		 * conflating them produced a genuinely absurd strip: from the letter,
		 * Account was clickable and Introductions was not — you could jump PAST a
		 * step you could not jump TO. `done` is "have you been here", which is
		 * about drawing; this is "are you entitled to be here", which is about
		 * gating, and the route computes it with the same predicate that guards
		 * the URL so the two can never disagree.
		 */
		open?: StepId[];
		/**
		 * Go back to a step already behind you.
		 *
		 * Optional so a surface can mount the strip as a pure indicator, but the
		 * page passes it everywhere — a progress bar that does nothing when you
		 * press it reads as broken, and the thing anyone actually wants from one
		 * is to go back and re-read a screen they clicked past.
		 */
		onjump?: (id: StepId) => void;
	}

	let { step, done = [], open, onjump }: Props = $props();

	const at = $derived(STEPS.findIndex((s) => s.id === step));

	// NEVER PAST A GATE. Moving ahead within what you are entitled to reach is
	// fine — Introductions asks for two optional names, and refusing to let
	// someone step to it is officious. What the strip must never do is offer a
	// screen the route would bounce: the account gate, or the reveal on a box
	// with nothing on it.
	const reachable = (id: StepId) =>
		Boolean(onjump) && id !== step && (open ?? done).includes(id);

	const label = $derived(STEPS[at]?.label ?? "");
</script>

<div class="head">
	<!-- THE NAME LIVES HERE, NOT IN THE STRIP.
	     It used to sit inside the current pill, in flow, so advancing a step took
	     the label off one icon and put it on another — which re-laid out the whole
	     row and read as a flick. Here it is left-anchored while the strip is
	     right-anchored, so the text can change length and nothing else moves at
	     all. The strip becomes six fixed icons that never resize. -->
	<p class="kicker">
		Virtues Onboarding
		<span class="sep" aria-hidden="true">/</span>
		<span class="here">{label}</span>
	</p>

	<nav class="prog" aria-label="Onboarding progress">
		{#each STEPS as s, i (s.id)}
			<button
				type="button"
				class="step"
				class:current={s.id === step}
				class:passed={i < at || done.includes(s.id)}
				class:live={reachable(s.id)}
				disabled={!reachable(s.id)}
				aria-current={s.id === step ? "step" : undefined}
				aria-label={s.label}
				onclick={() => reachable(s.id) && onjump?.(s.id)}
			>
				<Icon icon={s.icon} width="14" />

				<!-- EVERY OTHER LABEL FLOATS. Expanding a name inline on hover
				     re-flowed the whole strip — every icon to its right slid over,
				     under a cursor that was aiming at one of them. Absolute
				     positioning takes the label out of flow entirely, so a hover
				     moves nothing. No `title`, or the browser paints its own second
				     tooltip a second later. -->
				<span class="tip" aria-hidden="true">{s.label}</span>
			</button>
		{/each}
	</nav>
</div>

<style>
	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		/* The eyebrow anchors left, the strip rides the right edge, and the space
		   between them belongs to neither. `gap` is the floor for when the row
		   wraps on a narrow screen. */
		justify-content: space-between;
		gap: 0.5rem 2.5rem;
		/* The room the h1 beneath it needs to stop feeling crowded. */
		margin-bottom: 1.75rem;
	}

	.kicker {
		margin: 0;
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 11px;
		letter-spacing: 0.18em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
	}

	.prog {
		display: flex;
		align-items: center;
		gap: 0.2rem;
	}

	/* Every step is the same size, always. Nothing in this row resizes, so
	   nothing in it can shift. */
	.step {
		position: relative;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.75rem;
		height: 1.75rem;
		border: none;
		border-radius: 999px;
		background: none;
		color: var(--color-foreground-subtle);
		cursor: default;
		transition:
			background 0.18s ease,
			color 0.18s ease;
	}

	/* Only the steps you can actually return to take a pointer. A hand over a
	   step that does nothing is a promise the strip cannot keep. */
	.live {
		cursor: pointer;
	}

	.live:hover {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}

	/* Behind you: full ink, no fill. Ahead: subtle. The difference is weight
	   rather than a tick, because a tick on an optional step would be a lie. */
	.passed {
		color: var(--color-foreground-muted);
	}

	/* Marked by ink and a disc, not by growing. */
	.current {
		color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 12%, transparent);
	}

	.current:hover {
		color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 16%, transparent);
	}

	.sep {
		margin: 0 0.5rem;
		opacity: 0.4;
	}

	/* The one thing in the header that changes between screens, and it is
	   left-anchored so its width belongs to nobody else. */
	.here {
		color: var(--color-primary);
	}

	/* ── the hover label ────────────────────────────────────────────────
	   Absolutely positioned, so showing it costs the layout nothing. Below the
	   icon rather than above: the strip sits at the top of the screen, and a
	   tooltip above it would be clipped by the viewport. */
	.tip {
		position: absolute;
		top: calc(100% + 0.45rem);
		left: 50%;
		transform: translateX(-50%) translateY(-2px);
		z-index: 5;
		pointer-events: none;
		white-space: nowrap;
		padding: 0.3rem 0.5rem;
		border: 1px solid var(--color-border);
		border-radius: 7px;
		background: var(--color-surface-elevated, var(--color-background));
		box-shadow: 0 4px 14px rgb(0 0 0 / 0.09);
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 10px;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		color: var(--color-foreground-muted);
		opacity: 0;
		visibility: hidden;
		transition:
			opacity 0.1s ease,
			transform 0.1s ease,
			visibility 0s linear 0.1s;
	}

	/* No delay. The earlier quarter-second was there to stop labels firing as the
	   pointer swept the row — but that was when the label was IN FLOW and opening
	   one shoved the other icons sideways. A floating label costs the layout
	   nothing, so the twitch it was guarding against no longer exists, and the
	   delay only made the strip feel unresponsive. */
	.step:hover .tip,
	.step:focus-visible .tip {
		opacity: 1;
		visibility: visible;
		transform: translateX(-50%) translateY(0);
	}

	/* The last two sit near the right edge of a right-aligned strip, so a
	   centered label would hang off the page. Anchor them by their right edge
	   instead. */
	.step:nth-last-child(-n + 2) .tip {
		left: auto;
		right: 0;
		transform: translateY(-2px);
	}

	.step:nth-last-child(-n + 2):hover .tip,
	.step:nth-last-child(-n + 2):focus-visible .tip {
		transform: translateY(0);
	}

	@media (prefers-reduced-motion: reduce) {
		.tip {
			transition: none;
		}
	}
</style>
