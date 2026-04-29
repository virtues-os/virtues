<script lang="ts">
	import type { DataQuality } from '$lib/wiki/types/day';

	let { dataQuality }: { dataQuality: DataQuality } = $props();

	const dimensions = [
		{ key: 'who', label: 'Who' },
		{ key: 'whom', label: 'Whom' },
		{ key: 'what', label: 'What' },
		{ key: 'when', label: 'When' },
		{ key: 'where', label: 'Where' },
		{ key: 'why', label: 'Why' },
		{ key: 'how', label: 'How' },
	] as const;
</script>

<div class="coverage">
	<div class="coverage-grid">
		{#each dimensions as dim}
			{@const score = dataQuality.coverage[dim.key]}
			<div class="dim" title="{dim.label}: {score}/5">
				<span class="dim-label">{dim.label}</span>
				<span class="dim-dots">
					{#each [1, 2, 3, 4, 5] as i}
						<span class="dot" class:filled={i <= score}></span>
					{/each}
				</span>
			</div>
		{/each}
	</div>
	{#if dataQuality.note}
		<p class="coverage-note">{dataQuality.note}</p>
	{/if}
</div>

<style>
	.coverage {
		margin-top: 0.5rem;
	}

	.coverage-grid {
		display: grid;
		grid-template-columns: repeat(4, auto);
		gap: 0.25rem 1.5rem;
		width: fit-content;
	}

	.dim {
		display: flex;
		align-items: center;
		gap: 0.375rem;
	}

	.dim-label {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		min-width: 2.5rem;
	}

	.dim-dots {
		display: flex;
		gap: 2px;
	}

	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		border: 1px solid var(--color-foreground-subtle);
		opacity: 0.4;
	}

	.dot.filled {
		background: var(--color-foreground-subtle);
		opacity: 0.7;
	}

	.coverage-note {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		font-style: italic;
		margin: 0.5rem 0 0;
		max-width: 28rem;
	}
</style>
