<script lang="ts">
	/**
	 * ConnectionReceiptCard
	 *
	 * What the assistant changed in a connected service (`connection`), in
	 * the reply where it happened. One card per write, framed the way
	 * SentMessageCard frames a sent message, because it is the owner's
	 * account that changed. The box builds each receipt from the call it
	 * made and the service's answer (`tools/connection.rs`), not from the
	 * model's wording:
	 *
	 *   Save comment · Linear                     Open in Linear
	 *   VIR-412 Fix the sync
	 *
	 * Reads get no card. A call is a read only when its server labels the
	 * tool read-only, so a service that labels nothing shows a card for every
	 * call it makes.
	 */
	export interface Receipt {
		service: string;
		action: string;
		item: string | null;
		link: string | null;
	}

	let { receipts }: { receipts: Receipt[] } = $props();
</script>

{#each receipts as r, i (i)}
	<figure class="connection-receipt">
		<figcaption>
			<span class="headline">{r.action} · {r.service}</span>
			{#if r.link}
				<a class="link" href={r.link} target="_blank" rel="noreferrer">Open in {r.service}</a>
			{/if}
		</figcaption>
		{#if r.item}<p class="item">{r.item}</p>{/if}
	</figure>
{/each}

<style>
	.connection-receipt {
		margin: 0.75rem 0;
		max-width: 40em;
		padding: 0.75rem 1rem 0.875rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
	}

	.connection-receipt + .connection-receipt {
		margin-top: -0.25rem;
	}

	figcaption {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.125rem 1rem;
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.headline {
		min-width: 0;
		max-width: 100%;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.link {
		color: var(--color-primary);
		text-decoration: none;
		white-space: nowrap;
	}

	.link:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.item {
		margin: 0.375rem 0 0;
		font-family: var(--font-serif);
		font-size: 16px;
		line-height: 1.5;
		color: var(--color-foreground);
		overflow-wrap: anywhere;
	}
</style>
