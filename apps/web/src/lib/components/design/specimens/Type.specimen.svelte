<script lang="ts">
	/** The seven steps of the scale, each in the face it is used in, with invented sample text. */
	const STEPS = [
		{ token: '--text-title', face: 'serif', sample: 'Thursday, the ninth' },
		{ token: '--text-heading', face: 'serif', sample: 'Where the week went' },
		{ token: '--text-lead', face: 'serif', sample: 'You walked to the harbor before the rain started.' },
		{ token: '--text-body', face: 'serif', sample: 'The afternoon was quiet. Nick called about the boat, and the plan for Saturday held.' },
		{ token: '--text-label', face: 'sans', sample: 'Open sources' },
		{ token: '--text-small', face: 'sans', sample: 'Edited 9 October · 3 notes' },
		{ token: '--text-caption', face: 'sans', sample: 'Pinned' }
	];
</script>

<div class="steps">
	{#each STEPS as step (step.token)}
		<div class="step">
			<div class="meta"><code>{step.token}</code> <span>{step.face}</span></div>
			<div class="sample" style="font-size: var({step.token}); font-family: var(--font-{step.face})">{step.sample}</div>
		</div>
	{/each}
</div>

<style>
	.steps {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.step {
		display: grid;
		grid-template-columns: 160px minmax(0, 1fr);
		align-items: baseline;
		gap: 16px;
	}
	.meta {
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
	}
	.meta span {
		margin-left: 4px;
	}
	.sample {
		line-height: 1.3;
		color: var(--color-foreground);
	}
</style>
