<script lang="ts">
	// Empty state shown briefly when no tabs are open
	// A new chat tab is opened automatically by the space store
</script>

<div class="discovery-page">
	<div class="content">
		<p class="subtitle">Loading...</p>
	</div>
</div>

<style>
	.discovery-page {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		padding: 2rem;
		background: var(--color-background);
	}

	.content {
		text-align: center;
	}

	.subtitle {
		color: var(--color-foreground-muted);
		font-size: 0.85rem;
	}
</style>
