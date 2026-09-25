<!--
	Setup's progress, as the mark itself.

	A small ∴ at the top of every step. Its three dots are the three thirds
	of Setup, the grouping the close has always drawn: Welcome, the letter
	and the server are the left foot; Wi-Fi, the subscription and the names
	the right; the three about your life the apex. Premise, premise,
	therefore. Each dot fills as its third is done, and the one you are in
	breathes.

	It replaced nine dots and a "Welcome · next, Letter" label (2026-09-25):
	a progress bar in an admin tool's grammar, over a cold open. Arc shows no
	progress at all; this shows it in the brand's own shape, and the logo
	that opens Setup is the thing that fills and, at the close, becomes the
	app.

	WHERE IT COMES FROM. On Welcome there is no small mark: the big ∴ in the
	lockup is the mark. Leaving Welcome, the small one starts exactly over
	the big one (`from`, the big mark's ink box) and flies up to its place,
	so the logo visibly becomes the progress.

	THE CLOSE. At the end the mark comes down to the middle of the window,
	fills, and grows; the app opens beneath it.

	Pressing it lists the steps, the way the dots could be pressed: any step
	up to the first one still open can be revisited.
-->
<script lang="ts">
	import { tick } from "svelte";
	import type { SetupStep, SetupStepId } from "./setup.svelte";
	import { reducedMotion } from "./motion";

	interface Props {
		steps: SetupStep[];
		current: SetupStepId | null;
		closing?: boolean;
		/** Welcome draws its own, bigger mark. */
		hidden?: boolean;
		/** The big mark's ink box, to fly up from once. */
		from?: DOMRect | null;
		onpick: (id: SetupStepId) => void;
	}

	let { steps, current, closing = false, hidden = false, from = null, onpick }: Props = $props();

	/** The mark's dots on its 24-unit box, in thirds' order. */
	const DOTS = [
		{ cx: 4.5, cy: 18 },
		{ cx: 19.5, cy: 18 },
		{ cx: 12, cy: 5 },
	];
	const R = 3;
	const SIZE = 26;

	const third = (i: number) => Math.min(2, Math.floor((i * 3) / Math.max(1, steps.length)));
	const index = $derived(current ? steps.findIndex((s) => s.id === current) : -1);
	const here = $derived(index >= 0 ? third(index) : -1);
	/** How full each dot is: its third's done steps over its size. */
	const fill = $derived(
		[0, 1, 2].map((t) => {
			const mine = steps.filter((_, i) => third(i) === t);
			if (!mine.length) return 0;
			return mine.filter((s) => s.status === "done").length / mine.length;
		}),
	);
	const label = $derived(index >= 0 ? `${steps[index].label} · ${index + 1} of ${steps.length}` : "");

	let open = $state(false);
	let el = $state<HTMLElement | null>(null);

	// The flight up from the big mark, once.
	let flown = false;
	$effect(() => {
		if (!from || flown || !el) return;
		flown = true;
		void (async () => {
			await tick();
			if (!el || reducedMotion()) return;
			const to = el.querySelector("svg")!.getBoundingClientRect();
			const s = from.width / to.width;
			const dx = from.left + from.width / 2 - (to.left + to.width / 2);
			const dy = from.top + from.height / 2 - (to.top + to.height / 2);
			el.animate(
				[
					{ transform: `translate(calc(-50% + ${dx}px), ${dy}px) scale(${s})` },
					{ transform: "translate(-50%, 0) scale(1)" },
				],
				{ duration: 900, easing: "cubic-bezier(0.6, 0, 0.2, 1)" },
			);
		})();
	});

	function pick(id: SetupStepId) {
		open = false;
		onpick(id);
	}
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="mark" class:hidden class:closing bind:this={el}>
	<button
		type="button"
		class="glyph"
		aria-expanded={open}
		aria-label="Setup, {label || 'progress'}. Show the steps"
		onclick={() => (open = !open)}
		disabled={closing}
	>
		<svg viewBox="0 0 24 24" width={SIZE} height={SIZE} aria-hidden="true">
			{#each DOTS as d, t}
				<circle class="well" cx={d.cx} cy={d.cy} r={R} />
				<circle
					class="ink"
					class:here={t === here && !closing}
					cx={d.cx}
					cy={d.cy}
					r={R}
					style:transform="scale({closing ? 1 : Math.sqrt(fill[t])})"
				/>
			{/each}
		</svg>
		<span class="label">{label}</span>
	</button>

	{#if open}
		<ol class="list" role="list">
			{#each steps as step, i (step.id)}
				<li>
					<button
						type="button"
						class:current={step.id === current}
						disabled={!step.reachable || step.id === current}
						onclick={() => pick(step.id)}
					>
						<span class="state" data-status={step.status} aria-hidden="true"></span>
						<span class="n">{i + 1}</span>
						{step.label}
						{#if step.status === "skipped"}<span class="aside">skipped</span>{/if}
					</button>
				</li>
			{/each}
		</ol>
	{/if}
</div>

{#if open}
	<button type="button" class="scrim" aria-label="Close the steps" onclick={() => (open = false)}></button>
{/if}

<style>
	.mark {
		position: fixed;
		z-index: 60;
		left: 50%;
		top: max(18px, env(safe-area-inset-top));
		transform: translate(-50%, 0);
		display: flex;
		flex-direction: column;
		align-items: center;
		transition:
			opacity var(--m-base) var(--m-ease),
			top 1000ms cubic-bezier(0.6, 0, 0.2, 1),
			transform 1000ms cubic-bezier(0.6, 0, 0.2, 1);
	}
	.mark.hidden {
		opacity: 0;
		visibility: hidden;
	}

	.glyph {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		border: none;
		border-radius: 12px;
		background: none;
		color: var(--color-foreground);
		cursor: pointer;
	}
	.glyph:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}
	/* The label is there when asked for: on hover, on focus, with the list. */
	.label {
		font-size: 11.5px;
		letter-spacing: 0.01em;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
		opacity: 0;
		transform: translateY(-3px);
		transition:
			opacity var(--m-quick) ease,
			transform var(--m-base) var(--m-ease);
	}
	.glyph:hover .label,
	.glyph:focus-visible .label,
	.glyph[aria-expanded="true"] .label {
		opacity: 1;
		transform: none;
	}

	.well {
		/* Strong enough that an empty third still reads as a dot of the ∴,
		   not a gap in it. */
		fill: color-mix(in srgb, var(--color-foreground) 24%, transparent);
	}
	/* A dot fills by growing from its center: area, not radius, tracks the
	   third's progress (hence the square root). */
	.ink {
		fill: var(--color-foreground);
		transform-box: fill-box;
		transform-origin: center;
		transition: transform var(--m-slow) var(--m-spring);
	}
	/* The third you are in breathes: the dot with work left in it. */
	.ink.here {
		animation: breathe 3.2s ease-in-out infinite;
	}
	@keyframes breathe {
		50% {
			opacity: 0.55;
		}
	}

	/* The close: down to the middle, whole, and large. */
	.mark.closing {
		top: 50%;
		transform: translate(-50%, -50%) scale(4.5);
	}
	.mark.closing .label {
		opacity: 0 !important;
	}

	.list {
		position: absolute;
		top: calc(100% + 4px);
		min-width: 15rem;
		margin: 0;
		padding: 6px;
		list-style: none;
		border-radius: 14px;
		background: var(--color-surface-elevated, var(--color-surface));
		box-shadow:
			0 0 0 1px var(--color-border),
			0 12px 32px color-mix(in srgb, var(--color-foreground) 12%, transparent);
		animation: setup-rise var(--m-base) var(--m-ease) both;
		z-index: 2;
	}
	.list button {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 7px 10px;
		border: none;
		border-radius: 9px;
		background: none;
		font: inherit;
		font-size: 13.5px;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
	}
	.list button:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}
	.list button:disabled {
		cursor: default;
		color: var(--color-foreground-subtle);
	}
	.list button.current {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}
	.n {
		width: 1.1em;
		font-size: 11.5px;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}
	.state {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		box-shadow: inset 0 0 0 1.5px color-mix(in srgb, var(--color-foreground) 25%, transparent);
	}
	.state[data-status="done"] {
		background: var(--color-foreground);
		box-shadow: none;
	}
	.state[data-status="skipped"] {
		box-shadow: inset 0 0 0 1.5px var(--color-foreground);
	}
	.aside {
		margin-left: auto;
		font-size: 11.5px;
		color: var(--color-foreground-subtle);
	}
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 59;
		border: none;
		background: transparent;
	}

	@media (prefers-reduced-motion: reduce) {
		.mark,
		.ink {
			transition: none;
		}
		.ink.here {
			animation: none;
		}
	}
</style>
