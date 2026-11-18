<script lang="ts">
	/** Card as one padded surface, and as a list of rows divided by hairlines. */
	import Card from './Card.svelte';
</script>

<div class="grid">
	<Card>
		<div class="title">Backups</div>
		<div class="line">Your server copies itself every night at 3:00.</div>
	</Card>
	<Card list>
		{#each ['Calendar', 'Mail', 'Location'] as source (source)}
			<div class="row">
				<span>{source}</span>
				<span class="line">Connected</span>
			</div>
		{/each}
	</Card>
</div>

<style>
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
		gap: 24px;
		align-items: start;
	}
	.title {
		font-size: var(--text-label);
	}
	.line {
		margin-top: 4px;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
	}
	.row {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		padding: 12px 16px;
		font-size: var(--text-label);
	}
	.row .line {
		margin: 0;
	}
</style>
