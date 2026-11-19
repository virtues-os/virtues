<script lang="ts">
	/** Button in each variant at its one height, and its loading, disabled and icon states. */
	import Button from './Button.svelte';

	const VARIANTS = ['primary', 'secondary', 'ghost', 'danger'] as const;
</script>

<div class="rows">
	<div class="row">
		{#each VARIANTS as variant (variant)}
			<Button {variant}>{variant === 'danger' ? 'Delete page' : 'Open sources'}</Button>
		{/each}
	</div>
	<div class="row">
		<Button loading>Saving</Button>
		<Button disabled>Open sources</Button>
		<Button variant="secondary" icon="mynaui:plus">New page</Button>
	</div>
</div>

<style>
	.rows {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 12px;
	}
</style>
