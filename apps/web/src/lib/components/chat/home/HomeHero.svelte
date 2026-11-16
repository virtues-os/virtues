<script lang="ts">
	/** The top of home: the day and its part as a serif headline. */
	import { onMount } from 'svelte';

	let now = $state(new Date());
	onMount(() => {
		const t = setInterval(() => (now = new Date()), 60_000);
		return () => clearInterval(t);
	});

	const headline = $derived.by(() => {
		const day = now.toLocaleDateString(undefined, { weekday: 'long' });
		const h = now.getHours();
		const part = h >= 5 && h < 12 ? 'morning' : h >= 12 && h < 17 ? 'afternoon' : h >= 17 && h < 22 ? 'evening' : 'night';
		return `${day} ${part}`;
	});
</script>

<div class="home-hero">
	<h1 class="headline">{headline}</h1>
</div>

<style>
	.home-hero {
		width: min(var(--reading-measure, 38rem), calc(100% - 2 * var(--reading-gutter, 1.5rem)));
		display: flex;
		flex-direction: column;
		gap: 12px;
		pointer-events: auto;
	}

	.headline {
		margin: 0 0 0 2px;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 40px;
		line-height: 1.05;
		letter-spacing: -0.012em;
		color: var(--color-foreground);
	}

	@media (max-width: 768px), (pointer: coarse) {
		.headline {
			font-size: 30px;
		}
	}
</style>
