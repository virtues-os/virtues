<script lang="ts">
	/** The top of home: the day and its part as a serif headline. */
	import { onMount } from 'svelte';
	import { onScene } from '$lib/backgrounds/onScene.svelte';
	import { dayPartHeadline } from '$lib/utils/dateUtils';

	let now = $state(new Date());
	onMount(() => {
		const t = setInterval(() => (now = new Date()), 60_000);
		return () => clearInterval(t);
	});

	const headline = $derived(dayPartHeadline(now));
</script>

<div class="home-hero">
	<h1 class="headline" use:onScene>{headline}</h1>
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
		width: fit-content;
		margin: 0 0 0 2px;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 40px;
		line-height: 1.05;
		letter-spacing: -0.012em;
		color: var(--on-scene, var(--color-foreground));
		text-shadow: var(--on-scene-shade, none);
	}

	@media (max-width: 768px), (pointer: coarse) {
		.headline {
			font-size: 30px;
		}
	}
</style>
