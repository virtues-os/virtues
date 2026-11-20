<!--
	The placeholder a figure shows while it is drawn: while the show call
	streams (Show.svelte) and while a chart's module loads (ShowMark.svelte).
	Both use the chart height, so the reply doesn't jump when the chart lands.
-->
<script lang="ts">
	let { short = false }: { short?: boolean } = $props();
</script>

<div class="drawing" class:short></div>

<style>
	.drawing {
		height: 200px;
		border-radius: var(--radius-xl);
		background: linear-gradient(
			90deg,
			var(--color-surface) 0%,
			var(--color-surface-elevated) 50%,
			var(--color-surface) 100%
		);
		background-size: 200% 100%;
		animation: drawing 1.4s ease-in-out infinite;
	}
	.drawing.short {
		height: 64px;
	}
	@keyframes drawing {
		from {
			background-position: 100% 0;
		}
		to {
			background-position: -100% 0;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.drawing {
			animation: none;
		}
	}
</style>
