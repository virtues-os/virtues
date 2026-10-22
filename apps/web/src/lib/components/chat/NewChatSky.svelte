<script lang="ts">
	/**
	 * The empty new chat's opening: today as a small sky.
	 *
	 * Each of today's events is a dot, placed left to right by when it
	 * happened, so the sky fills in as the day goes on and the right side is
	 * the part of the day still to come. A dot's size follows how long the
	 * event lasted and its ink follows how unlike your usual it was.
	 *
	 * With no events, it falls back to the mark and word the screen always had.
	 */
	import { onMount } from 'svelte';
	import { fade } from 'svelte/transition';
	import type { ChatOpening } from '$lib/api/client';
	import { getDayEvents, type TemporalEventApi } from '$lib/wiki/api';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';

	// TODO(2026-10-22): delete with ChatView's `bind:opening` and its opening
	// state. The box no longer writes an opening sentence, so this stays null;
	// ChatView had another change open when the sentence was removed.
	let { opening = $bindable(null) }: { opening?: ChatOpening | null } = $props();

	const W = 560;
	const H = 120;
	const PAD = 10;

	let events = $state<TemporalEventApi[] | null>(null);

	onMount(() => {
		const date = getLocalDateSlug();
		void getDayEvents(date)
			.then((rows) => (events = rows))
			.catch(() => (events = []));
	});

	function hash(s: string): number {
		let h = 2166136261;
		for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
		// Mix the high bits too: ids that differ only in their last character
		// otherwise land a few pixels apart, on one line.
		h = Math.imul(h ^ (h >>> 16), 0x85ebca6b);
		h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
		h ^= h >>> 16;
		return (h >>> 0) / 4294967295;
	}

	type Dot = { id: string; x: number; y: number; r: number; ink: number; delay: number; twinkle: number };

	const dots = $derived.by((): Dot[] => {
		const shown = (events ?? []).filter((e) => !e.user_hidden && !e.is_unknown && !e.is_sleep);
		if (!shown.length) return [];
		const midnight = new Date();
		midnight.setHours(0, 0, 0, 0);
		const first = Math.min(...shown.map((e) => new Date(e.start_time).getTime()));
		// Six in the morning, or earlier if the day started earlier, to midnight.
		const from = Math.min(first, midnight.getTime() + 6 * 3600e3);
		const to = midnight.getTime() + 24 * 3600e3;
		return shown
			.slice()
			.sort((a, b) => a.start_time.localeCompare(b.start_time))
			.map((e, i) => {
				const start = new Date(e.start_time).getTime();
				const minutes = Math.max(1, (new Date(e.end_time).getTime() - start) / 60e3);
				const z = e.local_novelty_z ?? e.novelty_z;
				const t = Math.min(1, Math.max(0, (start - from) / (to - from)));
				return {
					id: e.id,
					x: PAD + t * (W - 2 * PAD),
					y: 16 + hash(e.id) * (H - 32),
					r: Math.min(4.6, 1.3 + 2.4 * Math.sqrt(minutes / 240)),
					ink: z === null ? 0.3 : z >= 1 ? 0.9 : 0.5,
					delay: Math.min(1200, i * 45),
					twinkle: 5 + hash(e.id + 't') * 7
				};
			});
	});

	const showSky = $derived(dots.length > 0);
	const showMark = $derived(events !== null && !showSky);
</script>

<div class="opening">
	{#if showSky}
		<svg class="sky" viewBox="0 0 {W} {H}" role="img" aria-label="Today, one dot for each event so far" in:fade={{ duration: 300 }}>
			{#each dots as d (d.id)}
				<g class="dot" style="--delay: {d.delay}ms; --twinkle: {d.twinkle}s">
					<circle cx={d.x} cy={d.y} r={d.r} fill="currentColor" fill-opacity={d.ink} />
				</g>
			{/each}
		</svg>
	{:else if showMark}
		<div class="mark" in:fade={{ duration: 200 }}>
			<svg class="init-mark" viewBox="0 0 12 10.5" width="19.87" height="17.39" fill="currentColor">
				<circle class="init-dot init-dot-1" cx="6" cy="2.4" r="1.5" />
				<circle class="init-dot init-dot-2" cx="2.6" cy="8.1" r="1.5" />
				<circle class="init-dot init-dot-3" cx="9.4" cy="8.1" r="1.5" />
			</svg>
			<span class="init-word">Virtues</span>
		</div>
	{/if}

</div>

<style>
	.opening {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 1.25rem;
		width: 100%;
		padding: 0 1rem;
		color: var(--color-foreground);
		pointer-events: none;
	}

	.sky {
		width: min(560px, 100%);
		height: auto;
		overflow: visible;
	}

	.mark {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.init-mark {
		overflow: visible;
	}
	.init-word {
		font-family: var(--font-serif);
		font-size: 1.375rem;
		font-weight: 400;
		letter-spacing: 0.03em;
		-webkit-text-stroke: 0.2px currentColor;
	}

	@media (prefers-reduced-motion: no-preference) {
		/* The day assembling: dots arrive in the order they happened. */
		.dot {
			animation:
				dot-arrive 0.6s cubic-bezier(0.22, 1, 0.36, 1) var(--delay) backwards,
				dot-twinkle var(--twinkle) ease-in-out calc(var(--delay) + 0.6s) infinite;
			transform-box: fill-box;
			transform-origin: center;
		}
		.init-dot {
			animation: init-dot-settle 0.55s cubic-bezier(0.22, 1, 0.36, 1) backwards;
			transform-origin: center;
			transform-box: fill-box;
		}
		.init-dot-1 {
			animation-delay: 0.15s;
		}
		.init-dot-2 {
			animation-delay: 0.32s;
		}
		.init-dot-3 {
			animation-delay: 0.49s;
		}
		.init-word {
			animation: init-word-rise 0.7s cubic-bezier(0.22, 1, 0.36, 1) 0.75s backwards;
		}
	}

	@keyframes dot-arrive {
		from {
			opacity: 0;
			transform: scale(0.2) translateY(4px);
		}
	}
	/* Quick to dim and slow to return, so it breathes rather than blinks. */
	@keyframes dot-twinkle {
		0%,
		100% {
			opacity: 1;
		}
		18% {
			opacity: 0.55;
		}
	}
	@keyframes init-dot-settle {
		from {
			opacity: 0;
			transform: scale(0.3) translateY(3px);
		}
	}
	@keyframes init-word-rise {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
	}
</style>
