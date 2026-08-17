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
  The current step carries its name; the rest are marks. Pressing one says what
  it is and nothing else — it is a legend, and a legend that navigated would be
  a promise about gating that onboarding cannot keep.

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
		/** Steps finished, for the ones behind the current position. */
		done?: StepId[];
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

	let { step, done = [], onjump }: Props = $props();

	const at = $derived(STEPS.findIndex((s) => s.id === step));

	// NEVER FORWARD. Jumping ahead would walk past the account gate, or open the
	// reveal on an empty box — the strip says where you are, and it must not
	// become a way of lying about it.
	const reachable = (id: StepId) => Boolean(onjump) && id !== step && done.includes(id);
</script>

<div class="head">
	<p class="kicker">Virtues Onboarding</p>

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
				title={s.label}
				onclick={() => reachable(s.id) && onjump?.(s.id)}
			>
				<Icon icon={s.icon} width="14" />
				<!-- Always rendered, so the name is on the accessibility tree even
				     while it is collapsed to nothing visually. -->
				<span class="name"><span class="name-inner">{s.label}</span></span>
			</button>
		{/each}
	</nav>
</div>

<style>
	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem 1rem;
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

	.step {
		display: inline-flex;
		align-items: center;
		padding: 0.3rem 0.45rem;
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

	.current {
		color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 11%, transparent);
	}

	.current:hover {
		color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 16%, transparent);
	}

	/* The expand. `max-width` rather than a grid trick because the label is one
	   line of known-short text and the eased width IS the mobile-tab feel. */
	.name {
		display: inline-block;
		max-width: 0;
		overflow: hidden;
		white-space: nowrap;
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 10.5px;
		letter-spacing: 0.13em;
		text-transform: uppercase;
		opacity: 0;
		/* Collapses immediately on leave; the DELAY lives on the open state
		   below. A label that fires the instant the cursor crosses a 22px target
		   fires constantly on the way to somewhere else, which is what made the
		   strip feel twitchy — five names flashing open as the pointer swept the
		   row. A quarter-second of intent filters that out entirely. */
		transition:
			max-width 0.28s cubic-bezier(0.2, 0.7, 0.2, 1),
			opacity 0.18s ease;
	}

	.name-inner {
		display: inline-block;
		padding-left: 0.45rem;
		padding-right: 0.15rem;
	}

	.live:hover .name,
	.step:focus-visible .name {
		max-width: 14rem;
		opacity: 1;
		transition-delay: 0.26s;
	}

	/* Where you are is not a hover state, so it carries no delay and never
	   collapses. */
	.current .name {
		max-width: 14rem;
		opacity: 1;
	}

	@media (prefers-reduced-motion: reduce) {
		.name {
			transition: none;
		}
	}
</style>
