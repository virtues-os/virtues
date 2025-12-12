<script lang="ts">
	export let exchanges: Array<{
		index: number;
		subject: string | undefined;
		userContent: string;
	}> = [];

	// Filter to only show exchanges with subjects
	$: exchangesWithSubjects = exchanges.filter((ex) => ex.subject);
</script>

{#if exchangesWithSubjects.length > 0}
	<nav class="table-of-contents">
		<div class="toc-items">
			{#each exchangesWithSubjects as exchange}
				<a href="#{`exchange-${exchange.index}`}" class="toc-item">
					{exchange.subject}
				</a>
			{/each}
		</div>
	</nav>
{/if}

<style>
	.table-of-contents {
		position: fixed;
		top: 1rem;
		left: 1rem;
		max-width: 16rem;
		z-index: 10;
	}

	.toc-items {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.toc-item {
		font-family: inherit;
		color: var(--color-foreground);
		font-size: 0.875rem;
		line-height: 1.5;
		text-decoration: none;
		transition:
			color 150ms ease,
			text-decoration-color 150ms ease;
		display: block;
	}

	.toc-item:hover {
		color: var(--color-primary);
		text-decoration: underline;
	}
</style>
