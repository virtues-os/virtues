<!--
	Hello — the cold open, before the letter.

	From Paul Henry's onboarding harness, Scene 0 (2026-09-23), then reworked
	the same day after Adam: "the pulse at the beginning is slow and a bit
	cliche", and the colors were wrong. What changed and why:

	- THE MARK FORMS AS AN ARGUMENT. The harness opened on one breathing drop
	  and a blue orb blooming out of it — the generic "AI is thinking" glow,
	  and three seconds of waiting before anything happened. The ∴ is logic
	  before it is a logo (voice.md), and the house already moves it that way
	  (the rail mark's hover, ThinkingMark): premise, premise, therefore. So
	  the two base dots land, then the apex, each with one ink ring that
	  spreads and dries. The whole mark is on the page in about a second.
	- THE THEME'S COLORS, NOT A FILM'S. The harness painted its own cream and
	  a bright cobalt; the letter that follows is the theme's paper and the
	  theme's primary, so the handoff changed color under the reader. Now the
	  page is `--color-surface` (what the onboarding layout paints) and the ink
	  is `--color-primary`, and the cut to the letter is invisible.
	- A LOCKUP, NOT A SWAP. Mark, wordmark, and line stay; the arrow
	  arrives at ~4s rather than ~11s.
	- ONE LINE (2026-09-25). The ∴ and "Virtues" sit side by side, the way
	  the logo is set everywhere else; stacked, the mark, word, line, button
	  and picture climbed most of the window. The mark still forms alone in
	  the middle, then steps left as the word resolves beside it.

	THE PLATE: a small panorama of a Tuscan town by a river, engraved in one
	ink, sitting on the foot of the page — the whole view, never cropped to
	fill, with the paper above it left open for the lockup. Our own drawing
	(generated 2026-09-25 through the house image slot, cut to a mask; the
	harness's picture was a screenshot of another studio's website). Inked in
	the theme's primary on either ground, a touch quieter on dark, where
	light line work on a dark ground reads louder. A first try (09-24)
	filled the window with a zoomed crop and fogged it around the words; it
	read as wallpaper and an effect, not a picture.

	ONE INK (2026-09-25). The mark, the wordmark and the line are all the
	page's text color; the theme's primary is left to the picture and the
	button. No italic either: the house serif is roman only.

	THE SOUND is Setup's, not this page's (setup/score.svelte.ts): the track
	starts here and carries on under the letter, and each dot of the mark
	lands on a soft note, premise, premise, therefore.

	Enter, Space, or → jumps to the settled frame; again goes on. Reduced
	motion gets the settled frame with no travel.
-->
<script lang="ts">
	import { onDestroy, onMount } from "svelte";
	import { getTheme, isThemeDark } from "$lib/utils/theme";
	import { score } from "$lib/components/setup/score.svelte";

	/**
	 * The door. Bare, it is the round arrow; with `continueLabel` it is a
	 * worded button ("Begin"). `onsettle` fires once the lockup is whole.
	 */
	let {
		onnext,
		onsettle,
		continueLabel,
	}: {
		onnext: () => void;
		onsettle?: () => void;
		continueLabel?: string;
	} = $props();

	/** How far the mark sits right of its place in the lockup, so it forms
	 *  alone in the middle of the window before the word arrives. */
	let rowEl = $state<HTMLElement | null>(null);
	let markEl = $state<SVGSVGElement | null>(null);
	let shift = $state(0);
	function measure() {
		if (!rowEl || !markEl) return;
		const row = rowEl.getBoundingClientRect();
		const mark = markEl.getBoundingClientRect();
		shift = row.left + row.width / 2 - (mark.left + mark.width / 2);
	}
	onMount(() => {
		measure();
		void document.fonts?.ready.then(measure);
		window.addEventListener("resize", measure);
		return () => window.removeEventListener("resize", measure);
	});

	/** The plate runs quieter on a dark ground, where line work reads loud. */
	let dark = $state(false);
	onMount(() => {
		dark = isThemeDark(getTheme());
		const on = () => (dark = isThemeDark(getTheme()));
		window.addEventListener("themechange", on);
		return () => window.removeEventListener("themechange", on);
	});



	/**
	 * The score, in ms from arrival. Premise, premise, therefore — the gaps
	 * are ThinkingMark's order, slowed to be read rather than glanced at.
	 */
	// The first dot lands at 150ms, over paper the route has already laid:
	// no blank first frame.
	const BEATS = { left: 150, right: 500, apex: 1050, word: 1650, line: 2550, door: 3500 };

	type Phase = "blank" | "left" | "right" | "apex" | "word" | "line" | "door";
	const ORDER: Phase[] = ["blank", "left", "right", "apex", "word", "line", "door"];

	let phase = $state<Phase>("blank");
	let still = $state(false);
	let leaving = $state(false);
	let timers: ReturnType<typeof setTimeout>[] = [];

	const at = (p: Phase) => ORDER.indexOf(phase) >= ORDER.indexOf(p);
	const later = (fn: () => void, ms: number) => timers.push(setTimeout(fn, ms));

	function go() {
		if (leaving) return;
		leaving = true;
		// At once: the small mark takes over from this one's ink box and
		// flies up (SetupMark), while the rest of the page crossfades.
		onnext();
	}

	/** The drawing leans a few pixels from the cursor once it has settled:
	 *  the page stays alive after the opening. Fine pointers only. */
	let lean = $state({ x: 0, y: 0 });
	function onMove(e: PointerEvent) {
		if (still || e.pointerType !== "mouse" || !at("door")) return;
		lean = { x: e.clientX / window.innerWidth - 0.5, y: e.clientY / window.innerHeight - 0.5 };
	}

	let settled = false;
	$effect(() => {
		if (phase === "door" && !settled) {
			settled = true;
			onsettle?.();
		}
	});

	function settle() {
		timers.forEach(clearTimeout);
		timers = [];
		phase = "door";
	}

	function onKey(e: KeyboardEvent) {
		if (e.key !== "Enter" && e.key !== " " && e.key !== "ArrowRight") return;
		// A focused control answers its own keys: Space on the light/dark
		// choice picks a theme, it does not leave.
		if (e.target instanceof HTMLElement && e.target.closest("button, input, select, textarea")) return;
		e.preventDefault();
		if (phase === "door") go();
		else settle();
	}

	onMount(() => {
		still = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
		score.start();
		if (still) {
			phase = "door";
			return;
		}
		for (const p of ["left", "right", "apex", "word", "line", "door"] as const) {
			later(() => (phase = p), BEATS[p]);
		}
		(["left", "right", "apex"] as const).forEach((p, i) => later(() => score.note(i), BEATS[p]));
	});

	onDestroy(() => {
		timers.forEach(clearTimeout);
	});
</script>

<svelte:window onpointerdown={() => score.start()} onpointermove={onMove} onkeydown={onKey} />

<div class="hello" class:still class:leaving class:settled={at("door")} role="presentation">
	<div class="group">
	<div class="lockup">
		<div class="row" bind:this={rowEl} class:joined={at("word")} style:--shift="{shift}px">
		<!-- The mark on its own 24-unit grid, the app icon's exactly, padded so
		     the ink rings have somewhere to spread. -->
		<svg class="mark" bind:this={markEl} viewBox="-8 -8 40 40" aria-hidden="true">
			<g class="dot left" class:in={at("left")}>
				<circle class="ring" cx="4.5" cy="18" r="2.85" />
				<circle class="ink" cx="4.5" cy="18" r="2.85" />
			</g>
			<g class="dot right" class:in={at("right")}>
				<circle class="ring" cx="19.5" cy="18" r="2.85" />
				<circle class="ink" cx="19.5" cy="18" r="2.85" />
			</g>
			<g class="dot apex" class:in={at("apex")}>
				<circle class="ring" cx="12" cy="5" r="2.85" />
				<circle class="ink" cx="12" cy="5" r="2.85" />
			</g>
		</svg><span class="word" class:in={at("word")}>Virtues</span>
		</div>
		<p class="line" class:in={at("line")}>
			Intelligence, made personal.
		</p>

		<div class="door" class:in={at("door") && !leaving}>
			{#if continueLabel}
				<button type="button" class="worded" onclick={go}>
					{continueLabel}
					<svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true">
						<path d="M5 12h13M13 6.5 18.5 12 13 17.5" />
					</svg>
				</button>
			{:else}
				<button type="button" class="round" onclick={go} aria-label="Continue to the letter">
					<svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
						<path d="M5 12h13M13 6.5 18.5 12 13 17.5" />
					</svg>
				</button>
			{/if}
		</div>
	</div>

	<div
		class="plate"
		class:in={at("apex")}
		class:dark
		style:--lx="{lean.x * -12}px"
		style:--ly="{lean.y * -6}px"
		aria-hidden="true"
	>
		<div class="drawing"></div>
	</div>
	</div>

	<h1 class="sr-only">Virtues. Intelligence, made personal.</h1>
</div>

<style>
	.hello {
		position: fixed;
		inset: 0;
		z-index: 50;
		display: grid;
		place-items: center;
		padding: 0 16px;
		overflow: hidden;
		/* The route's stage paints the paper and grain; this layer is ink. */
		background: transparent;
		color: var(--color-foreground);
		transition: opacity var(--m-quick, 180ms) ease;
	}
	.hello.leaving {
		opacity: 0;
	}
	/* Its ink box is the small mark now (SetupMark flies up from it). */
	.hello.leaving .mark {
		visibility: hidden;
	}

	/* ONE COMPOSITION (2026-09-25). The lockup, the door and the drawing
	   are one group centered in the window, the drawing as the ground the
	   door stands on. Before, the lockup floated in the top third and the
	   drawing was pinned to the bottom edge, with dead paper between. */
	.group {
		display: flex;
		flex-direction: column;
		align-items: center;
	}


	.lockup {
		position: relative;
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
	}

	/* ── the mark ─────────────────────────────────────────────────────── */

	/* THE ROW. Sized in the wordmark's em, so the mark scales with the word,
	   and set to the word's own lines: the apex's top on the capitals' top,
	   the feet on the baseline. The dots are r 2.85 on the mark's 24-unit
	   box, so the ink spans 18.7 units top to bottom; JJannon's capital
	   height is 0.676em (measured), so the 40-unit svg (padded for the
	   rings) is 0.676 × 40 / 18.7 = 1.446em square. Its padding is pulled in
	   by negative margins. The row is one line of text with the mark set
	   inline in it (no whitespace between them in the markup): an inline
	   svg stands on the baseline by its bottom MARGIN edge, so pulling that
	   margin up to the feet stands the feet on the word's baseline. (A flex
	   row aligns an svg by its box edge instead, and floated the mark.) */
	.row {
		white-space: nowrap;
		line-height: 1;
		font-size: clamp(3rem, 8vw, 5.5rem);
	}
	.mark {
		display: inline-block;
		vertical-align: baseline;
		width: 1.446em;
		height: 1.446em;
		/* Padding in em: left/right (1.65 + 8) / 40, top (2.15 + 8) / 40,
		   bottom (32 - 20.85) / 40, each × 1.446. Right keeps 0.22em of air. */
		margin: -0.367em -0.129em -0.403em -0.349em;
		overflow: visible;
		transform: translateX(var(--shift));
		transition: transform 1100ms cubic-bezier(0.6, 0, 0.2, 1);
	}
	.row.joined .mark {
		transform: none;
	}
	.dot .ink {
		fill: var(--color-foreground);
		transform-box: fill-box;
		transform-origin: center;
		transform: scale(0);
	}
	.dot .ring {
		fill: none;
		stroke: var(--color-foreground);
		stroke-width: 0.6;
		opacity: 0;
		transform-box: fill-box;
		transform-origin: center;
	}
	/* Landing: the dot arrives a touch large and wet, and settles to size. */
	.dot.in .ink {
		animation: land 620ms cubic-bezier(0.2, 0.9, 0.25, 1.2) forwards;
	}
	/* The ring: one spread of ink outward, drying as it goes. */
	.dot.in .ring {
		animation: spread 1100ms cubic-bezier(0.1, 0.6, 0.3, 1) forwards;
	}
	/* Therefore lands with more weight than either premise. */
	.dot.apex.in .ring {
		animation-duration: 1500ms;
		stroke-width: 0.8;
	}
	@keyframes land {
		0% {
			transform: scale(0);
			opacity: 0.4;
		}
		60% {
			transform: scale(1.16);
			opacity: 1;
		}
		100% {
			transform: scale(1);
			opacity: 1;
		}
	}
	@keyframes spread {
		0% {
			transform: scale(1);
			opacity: 0.55;
		}
		100% {
			transform: scale(4.2);
			opacity: 0;
		}
	}

	/* ── the plate ────────────────────────────────────────────────────── */
	/* The whole panorama, at a fixed modest width, standing on the foot of
	   the page with open paper above it. No fog: the drawing trails off on
	   its own. It rises into place as the mark completes. */
	/* It draws itself in from the left with the signature's pen (a soft
	   gradient mask slid across the drawing's own mask), then leans a few
	   pixels away from the cursor. */
	/* Two boxes, two masks: the pen (a soft gradient) on the outer, the
	   drawing's own mask on the inner. Both on one element left a hairline
	   along the top edge where the layers composited. */
	.plate {
		margin-top: clamp(1.5rem, 6vh, 4rem);
		width: min(980px, 88vw);
		aspect-ratio: 2640 / 716;
		pointer-events: none;
		-webkit-mask-image: linear-gradient(90deg, #000 44%, transparent 56%);
		mask-image: linear-gradient(90deg, #000 44%, transparent 56%);
		-webkit-mask-repeat: no-repeat;
		mask-repeat: no-repeat;
		-webkit-mask-size: 230% 100%;
		mask-size: 230% 100%;
		-webkit-mask-position: 100% 0;
		mask-position: 100% 0;
		opacity: 0.85;
		transform: translate(var(--lx, 0), var(--ly, 0));
		transition:
			-webkit-mask-position 3.2s cubic-bezier(0.45, 0.1, 0.3, 1),
			mask-position 3.2s cubic-bezier(0.45, 0.1, 0.3, 1),
			transform 1.6s var(--m-ease, ease);
	}
	.plate.in {
		-webkit-mask-position: 0 0;
		mask-position: 0 0;
	}
	.drawing {
		width: 100%;
		height: 100%;
		background: var(--color-primary);
		-webkit-mask: url("/onboarding/tuscany.webp") center / contain no-repeat;
		mask: url("/onboarding/tuscany.webp") center / contain no-repeat;
	}
	.plate.dark {
		opacity: 0.6;
	}

	/* Settled, the mark keeps breathing: each dot in turn, barely. */
	.settled .dot {
		transform-box: fill-box;
		transform-origin: center;
		animation: idle 4.8s ease-in-out infinite;
	}
	.settled .dot.right {
		animation-delay: 0.6s;
	}
	.settled .dot.apex {
		animation-delay: 1.2s;
	}
	@keyframes idle {
		50% {
			transform: scale(1.08);
		}
	}

	/* ── the words ────────────────────────────────────────────────────── */

	.word,
	.line {
		margin: 0;
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		font-style: normal;
		opacity: 0;
		filter: blur(8px);
		transform: translateY(6px);
		transition:
			opacity 900ms ease,
			filter 1100ms ease,
			transform 1100ms cubic-bezier(0.2, 0.7, 0.2, 1),
			letter-spacing 1400ms cubic-bezier(0.2, 0.7, 0.2, 1);
	}
	.word.in,
	.line.in {
		opacity: 1;
		filter: blur(0);
		transform: none;
	}
	/* The wordmark resolves out of the mark's side: it starts tucked a
	   little toward the ∴ and blurred, and comes to rest beside it. Its
	   spacing stays fixed so the mark's resting place is known before the
	   word arrives. */
	.word {
		display: inline-block;
		font-size: 1em;
		line-height: 1;
		letter-spacing: -0.005em;
		color: var(--color-foreground);
		transform: translateX(-0.25em);
		transition-delay: 250ms;
	}
	.line {
		margin-top: 1.4rem;
		font-size: clamp(1.15rem, 2.2vw, 1.6rem);
		color: var(--color-foreground);
	}

	/* ── the door ─────────────────────────────────────────────────────── */

	.door {
		margin-top: 2.5rem;
		opacity: 0;
		transform: translateY(6px);
		pointer-events: none;
		transition:
			opacity 700ms ease,
			transform 700ms ease;
	}
	.door {
		display: flex;
		flex-direction: column;
		align-items: center;
	}
	.door button.worded {
		display: inline-flex;
		align-items: center;
		gap: 0.55rem;
		width: auto;
		height: auto;
		padding: 0.95rem 2rem;
		border-radius: 999px;
		font: inherit;
		font-family: var(--font-sans, system-ui);
		font-size: 17px;
	}
	.door.in {
		opacity: 1;
		transform: none;
		pointer-events: auto;
	}
	.door button {
		display: grid;
		place-content: center;
		width: 52px;
		height: 52px;
		border-radius: 50%;
		border: none;
		background: var(--color-primary);
		color: var(--color-background);
		cursor: pointer;
		transition:
			transform 0.2s ease,
			opacity 0.2s ease;
	}
	/* Hover dims, like every other button in Setup; nothing moves. */
	.door button:hover {
		opacity: 0.86;
	}
	.door button:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 3px;
	}
	.door svg {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.6;
		stroke-linecap: round;
		stroke-linejoin: round;
	}


	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}

	/* Reduced motion: the settled frame, nothing travels. */
	.hello.still .dot .ink {
		transform: none;
	}
	@media (prefers-reduced-motion: reduce) {
		.hello *,
		.hello {
			animation: none !important;
			transition: none !important;
		}
		.dot .ink {
			transform: none;
		}
		.dot .ring {
			opacity: 0;
		}
	}
</style>
