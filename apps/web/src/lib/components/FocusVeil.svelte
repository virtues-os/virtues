<script lang="ts" module>
	import { cubicIn, cubicOut, expoOut } from 'svelte/easing';
	import { reducedMotion } from '$lib/stores/motion.svelte';

	const FOCUS_MS = 290;
	const RELEASE_MS = 210;
	const CARD_OUT_MS = 145;
	/** Handing off to a page: the card drops, the veil fades without its blur moving. */
	const HANDOFF_CARD_MS = 160;
	const HANDOFF_VEIL_MS = 260;
	/** The window's blur behind the card, in px. */
	const VEIL_BLUR = 10;

	const still = () => reducedMotion();

	/** The card comes sharp out of a blur, settling from 104% to its size. */
	export function cardIn(_node: HTMLElement) {
		if (still()) return { duration: 160, css: (t: number) => `opacity: ${t}` };
		return {
			duration: FOCUS_MS,
			easing: expoOut,
			css: (t: number) => `opacity: ${t}; filter: blur(${(1 - t) * 10}px); transform: scale(${1.04 - 0.04 * t})`,
		};
	}

	/** The card blurs and swells a little as it fades. Sinking, it drops away as a page comes up. */
	export function cardOut(_node: HTMLElement, opts: { sinking?: boolean } = {}) {
		if (still()) return { duration: 120, css: (t: number) => `opacity: ${t}` };
		if (opts.sinking) {
			return {
				duration: HANDOFF_CARD_MS,
				easing: cubicIn,
				css: (t: number) => `opacity: ${t}; transform: translateY(${(1 - t) * 18}px) scale(${0.96 + 0.04 * t})`,
			};
		}
		return {
			duration: CARD_OUT_MS,
			easing: cubicIn,
			css: (t: number) => `opacity: ${t}; filter: blur(${(1 - t) * 8}px); transform: scale(${1.03 - 0.03 * t})`,
		};
	}
</script>

<script lang="ts">
	/**
	 * The veil a focused card opens over, as ⌘K's search does: the window
	 * behind goes soft and fades into a fine dot screen, clear around the
	 * card and denser toward the edges. The card inside uses `cardIn` and
	 * `cardOut` from this file, so it comes sharp out of a blur as the
	 * window loses focus, like a camera pulling focus. Closing pulls focus
	 * back to the page, faster than it came.
	 *
	 * The blur radius itself animates, which repaints the window each frame
	 * for those few hundred milliseconds; a fade between sharp and blurred
	 * reads as a dissolve instead of a change of focus. The phone and reduced
	 * transparency draw the veil without a blur.
	 */
	import type { Snippet } from 'svelte';
	import type { Action } from 'svelte/action';
	import { portal as toBody } from '$lib/actions/portal';

	let {
		open,
		onBackdrop,
		portal = false,
		sinking = () => false,
		children,
	}: {
		open: boolean;
		/** A click on the veil itself, outside the card. */
		onBackdrop: () => void;
		/** Move the veil to `<body>`, for a host declared inside the page. */
		portal?: boolean;
		/** Read as it closes: true when the card hands off to a page, so the veil only fades. */
		sinking?: () => boolean;
		children: Snippet;
	} = $props();

	/** Moves the veil to `<body>` when `on`, and takes it away with the veil. */
	const lift: Action<HTMLElement, boolean> = (node, on) => {
		if (!on) return;
		const moved = toBody(node);
		return { destroy: () => moved?.destroy?.() };
	};

	const flat = () => window.matchMedia('(hover: none), (prefers-reduced-transparency: reduce)').matches;

	const veilAt = (e: number) => {
		if (flat()) return `opacity: ${e}`;
		const filter = `blur(${VEIL_BLUR * e}px) saturate(${1 - 0.1 * e})`;
		return `opacity: ${e}; backdrop-filter: ${filter}; -webkit-backdrop-filter: ${filter}`;
	};

	function veilIn(_node: HTMLElement) {
		if (still()) return { duration: 160, css: (t: number) => `opacity: ${t}` };
		return { duration: FOCUS_MS + 15, easing: expoOut, css: (t: number) => veilAt(t) };
	}

	/** Svelte runs an outro from t = 1 down to 0. */
	function veilOut(_node: HTMLElement) {
		if (sinking()) return { duration: HANDOFF_VEIL_MS, easing: cubicOut, css: (t: number) => `opacity: ${t}` };
		if (still()) return { duration: 120, css: (t: number) => `opacity: ${t}` };
		return { duration: RELEASE_MS, easing: cubicOut, css: (t: number) => veilAt(t) };
	}
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div class="veil" onclick={(e) => e.target === e.currentTarget && onBackdrop()} use:lift={portal} in:veilIn out:veilOut>
		{@render children()}
	</div>
{/if}

<style>
	/* Centred on the window, with the window's air on both sides. */
	.veil {
		position: fixed;
		inset: 0;
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding: calc(var(--chrome-row-h) + 8vh) var(--window-air) 16px;
		z-index: var(--z-modal);
		background: color-mix(in srgb, var(--color-background) 28%, transparent);
		backdrop-filter: blur(10px) saturate(0.9);
		-webkit-backdrop-filter: blur(10px) saturate(0.9);
	}

	/* The dot screen: dots of ink 4px apart, masked so the card's
	   surroundings stay clear and the screen thickens toward the edges. */
	.veil::before {
		content: '';
		position: absolute;
		inset: 0;
		pointer-events: none;
		background-image: radial-gradient(
			color-mix(in srgb, var(--color-foreground) 22%, transparent) 0.75px,
			transparent 1.25px
		);
		background-size: 4px 4px;
		mask-image: radial-gradient(ellipse 70% 75% at 50% 30%, transparent 25%, var(--color-foreground) 100%);
	}

	@media (prefers-reduced-transparency: reduce) {
		.veil {
			backdrop-filter: none;
			-webkit-backdrop-filter: none;
			background: color-mix(in srgb, var(--color-background) 70%, transparent);
		}
	}

	@media (hover: none) {
		.veil {
			/* Clears the notch on top, and at the bottom the home indicator and
			   the keyboard this raises the moment it opens. `--keyboard-inset`
			   is 0 anywhere there isn't one. */
			padding: max(6vh, env(safe-area-inset-top)) 16px
				max(16px, env(safe-area-inset-bottom), var(--keyboard-inset, 0px));
			background: rgba(0, 0, 0, 0.4);
			backdrop-filter: none;
			-webkit-backdrop-filter: none;
		}

		.veil::before {
			display: none;
		}
	}
</style>
