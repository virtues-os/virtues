<script lang="ts">
	/** Button in each variant and size, and its loading and disabled states. */
	import Button from './Button.svelte';

	const VARIANTS = ['primary', 'secondary', 'ghost', 'danger'] as const;
	const SIZES = ['sm', 'md', 'lg'] as const;
</script>

<div class="grid">
	{#each VARIANTS as variant (variant)}
		<div class="label">{variant}</div>
		{#each SIZES as size (size)}
			<div><Button {variant} {size}>Open sources</Button></div>
		{/each}
	{/each}
	<div class="label">states</div>
	<div><Button loading>Saving</Button></div>
	<div><Button disabled>Open sources</Button></div>
	<div><Button variant="secondary" icon="mynaui:plus">New page</Button></div>
</div>

<style>
	.grid {
		display: grid;
		grid-template-columns: 96px repeat(3, max-content);
		align-items: center;
		gap: 12px 16px;
	}
	.label {
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
	}
</style>
