<script lang="ts">
	import { onMount } from "svelte";

	interface Props {
		label: string;
	}

	let { label }: Props = $props();

	// Track displayed label and transition state
	let displayLabel = $state("");
	let isTransitioning = $state(false);
	let initialized = $state(false);

	// Animated dots that cycle: . → .. → ...
	let dots = $state("");

	onMount(() => {
		// Initialize with first label
		displayLabel = label;
		initialized = true;

		const interval = setInterval(() => {
			dots = dots.length >= 3 ? "" : dots + ".";
		}, 500);

		return () => clearInterval(interval);
	});

	// Handle label changes with y-slide animation
	$effect(() => {
		if (initialized && label !== displayLabel && !isTransitioning) {
			isTransitioning = true;
			// After exit animation, swap label and animate in
			setTimeout(() => {
				displayLabel = label;
				setTimeout(() => {
					isTransitioning = false;
				}, 200);
			}, 150);
		}
	});
</script>

<div class="thinking-indicator">
	<span class="label-container" class:transitioning={isTransitioning}>
		<span class="shimmer-text text-base">
			{displayLabel}{dots}
		</span>
	</span>
</div>

<style>
	.thinking-indicator {
		position: relative;
		padding: 0.75rem 0; /* Match message-wrapper: py-3 */
		opacity: 0;
		animation: fadeIn 0.3s ease-out forwards;
		overflow: hidden;
	}

	@keyframes fadeIn {
		from {
			opacity: 0;
			transform: translateY(4px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}

	/* Label container for y-slide transition */
	.label-container {
		display: inline-block;
		transition:
			transform 0.15s ease-out,
			opacity 0.15s ease-out;
	}

	.label-container.transitioning {
		transform: translateY(-8px);
		opacity: 0;
	}

	/* Shimmer text effect */
	.shimmer-text {
		background: linear-gradient(
			90deg,
			var(--color-foreground-subtle) 0%,
			var(--color-foreground-subtle) 35%,
			var(--color-foreground) 50%,
			var(--color-foreground-subtle) 65%,
			var(--color-foreground-subtle) 100%
		);
		background-size: 200% 100%;
		-webkit-background-clip: text;
		background-clip: text;
		color: transparent;
		animation: shimmer 2.5s linear infinite;
	}

	@keyframes shimmer {
		0% {
			background-position: 100% 0;
		}
		100% {
			background-position: -100% 0;
		}
	}

	/* Respect reduced motion preferences */
	@media (prefers-reduced-motion: reduce) {
		.thinking-indicator {
			animation: none;
			opacity: 1;
		}

		.label-container {
			transition: none;
		}

		.shimmer-text {
			animation: none;
			color: var(--color-foreground-subtle);
			background: none;
		}
	}
</style>
