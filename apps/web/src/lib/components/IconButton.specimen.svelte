<script lang="ts">
	/** IconButton in each size and variant, and as a pressed toggle. */
	import IconButton from './IconButton.svelte';

	const SIZES = ['xs', 'sm', 'md', 'touch'] as const;
	const VARIANTS = ['ghost', 'secondary', 'danger'] as const;
	let pinned = $state(true);
</script>

<div class="grid">
	<div></div>
	{#each SIZES as size (size)}<div class="label">{size}</div>{/each}
	{#each VARIANTS as variant (variant)}
		<div class="label">{variant}</div>
		{#each SIZES as size (size)}
			<div>
				<IconButton icon={variant === 'danger' ? 'mynaui:trash' : 'mynaui:dots'} label="More" {size} {variant} />
			</div>
		{/each}
	{/each}
	<div class="label">pressed</div>
	<div>
		<IconButton icon="mynaui:pin" label={pinned ? 'Unpin' : 'Pin'} pressed={pinned} onclick={() => (pinned = !pinned)} />
	</div>
</div>

<style>
	.grid {
		display: grid;
		grid-template-columns: 96px repeat(4, 56px);
		align-items: center;
		gap: 12px;
	}
	.label {
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
	}
</style>
