<script lang="ts">
	/** Badge in each variant, tinted and outlined. */
	import Badge from './Badge.svelte';

	const VARIANTS = ['muted', 'primary', 'success', 'warning', 'error', 'info'] as const;
</script>

<div class="rows">
	<div class="row">
		{#each VARIANTS as variant (variant)}<Badge {variant}>{variant}</Badge>{/each}
	</div>
	<div class="row">
		{#each VARIANTS as variant (variant)}<Badge {variant} outline>{variant}</Badge>{/each}
	</div>
</div>

<style>
	.rows {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
</style>
