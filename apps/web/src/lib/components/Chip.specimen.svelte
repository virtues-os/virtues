<script lang="ts">
	/** Chip as a row of filters, as links and as labels. */
	import Chip from './Chip.svelte';

	const FILTERS = ['All', 'Instagram', 'X', 'Browser'] as const;
	let picked = $state<string>('All');
</script>

<div class="rows">
	<div class="row">
		{#each FILTERS as f (f)}
			<Chip selected={picked === f} onclick={() => (picked = f)}>{f}</Chip>
		{/each}
	</div>
	<div class="row">
		<Chip href="#chip">Kitchen remodel</Chip>
		<Chip>Tucson</Chip>
		<Chip>zellige</Chip>
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
