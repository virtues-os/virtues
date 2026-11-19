<script lang="ts">
	/** The resting fill and the four interaction steps, on paper and on the desk. Point at and press the last row to feel the timing. */
	const STEPS = [
		{ name: 'Rest', token: '' },
		{ name: 'Fill', token: '--fill-bg' },
		{ name: 'Hover', token: '--hover-bg' },
		{ name: 'Selected', token: '--active-bg' },
		{ name: 'Press', token: '--press-bg' }
	];
</script>

<div class="grounds">
	{#each ['--page', '--desk'] as ground (ground)}
		<div class="ground" style="background: var({ground})">
			<div class="ground-name"><code>{ground}</code></div>
			{#each STEPS as step (step.name)}
				<div class="row" style="background: {step.token ? `var(${step.token})` : 'transparent'}">
					{step.name}{#if step.token} <code>{step.token}</code>{/if}
				</div>
			{/each}
			<button type="button" class="row live">Point at me, then press</button>
		</div>
	{/each}
</div>

<style>
	.grounds {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
		gap: 16px;
	}
	.ground {
		padding: 12px;
		border-radius: var(--radius-xl);
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
	}
	.ground-name {
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
		margin-bottom: 8px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 32px;
		padding: 0 12px;
		margin-bottom: 4px;
		border-radius: var(--radius-md);
		font-size: var(--text-label);
		color: var(--color-foreground);
	}
	.row code {
		font-size: var(--text-caption);
		color: var(--color-foreground-muted);
	}
	.live {
		width: 100%;
		border: none;
		background: transparent;
		font: inherit;
		font-size: var(--text-label);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.live:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.live:active {
		background: var(--press-bg);
		transition-duration: var(--motion-press);
	}
	@media (prefers-reduced-motion: reduce) {
		.live {
			transition: none;
		}
	}
</style>
