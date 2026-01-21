<script lang="ts">
	import { slide } from "svelte/transition";
	import type { ContextVector } from "$lib/wiki";
	import { computeCompleteness } from "$lib/wiki";

	interface Props {
		contextVector: ContextVector;
	}

	let { contextVector }: Props = $props();

	let expanded = $state(false);

	const completeness = $derived(
		Math.round(computeCompleteness(contextVector) * 100),
	);

	const dimensions = $derived([
		{ key: "when", label: "When", value: contextVector.when },
		{ key: "where", label: "Where", value: contextVector.where },
		{ key: "who", label: "Who", value: contextVector.who },
		{ key: "what", label: "What", value: contextVector.what },
		{ key: "why", label: "Why", value: contextVector.why },
		{ key: "how", label: "How", value: contextVector.how },
	]);
</script>

<div class="context-vector">
	<button class="toggle-btn" onclick={() => (expanded = !expanded)}>
		<span class="toggle-label">Completeness · {completeness}%</span>
		<iconify-icon
			icon={expanded ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line"}
		></iconify-icon>
	</button>

	{#if expanded}
		<div class="dimensions" transition:slide={{ duration: 200 }}>
			<!-- Main completeness bar -->
			<div class="main-bar">
				<div class="bar-fill" style="width: {completeness}%"></div>
			</div>

			<!-- All 6 dimensions in one row -->
			<div class="dimension-row">
				{#each dimensions as dim}
					{@const pct = Math.round(dim.value * 100)}
					<div class="dimension">
						<div class="dim-header">
							<span class="dim-label">{dim.label}</span>
							<span class="dim-value">{pct}%</span>
						</div>
						<div class="dim-bar">
							<div class="dim-fill" style="width: {pct}%"></div>
						</div>
					</div>
				{/each}
			</div>
		</div>
	{/if}
</div>

<style>
	.context-vector {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		width: 100%;
	}

	.toggle-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0;
		background: none;
		border: none;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		align-self: flex-start;
	}

	.toggle-btn:hover {
		color: var(--color-foreground-muted);
	}

	.toggle-label {
		font-size: 0.8125rem;
	}

	.dimensions {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		width: 100%;
	}

	.main-bar {
		height: 10px;
		background-image: radial-gradient(
			color-mix(in srgb, var(--color-foreground) 20%, transparent) 1px,
			transparent 0
		);
		background-size: 6px 6px;
		border-radius: 2px;
		overflow: hidden;
	}

	.bar-fill {
		height: 100%;
		background: var(--color-foreground-muted);
		border-radius: 2px;
	}

	.dimension-row {
		display: flex;
		gap: 1rem;
	}

	.dimension {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.dim-header {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
	}

	.dim-label {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.dim-value {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.dim-bar {
		height: 10px;
		background-image: radial-gradient(
			color-mix(in srgb, var(--color-foreground) 20%, transparent) 1px,
			transparent 0
		);
		background-size: 6px 6px;
		border-radius: 2px;
		overflow: hidden;
	}

	.dim-fill {
		height: 100%;
		background: var(--color-foreground-muted);
		border-radius: 2px;
	}
</style>
