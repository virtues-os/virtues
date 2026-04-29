<script lang="ts">
	import { listSourceCatalog, type SourceCatalogItem } from '$lib/api/client';
	import ProviderTile from './ProviderTile.svelte';

	let {
		onConnect
	}: {
		onConnect: (source: SourceCatalogItem) => void;
	} = $props();

	let sources = $state<SourceCatalogItem[]>([]);
	let loading = $state(true);
	let err = $state<string | null>(null);

	async function load() {
		loading = true;
		err = null;
		try {
			sources = await listSourceCatalog();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
	});

	export function refresh() {
		void load();
	}
</script>

<section class="catalog">
	<header>
		<h3>Available sources</h3>
		<p>Click a tile to connect a source.</p>
	</header>

	{#if err}
		<div class="error">{err}</div>
	{/if}

	{#if loading && sources.length === 0}
		<p class="muted">Loading…</p>
	{:else if sources.length === 0}
		<p class="muted">No sources registered.</p>
	{:else}
		<div class="grid">
			{#each sources as source (source.id)}
				<ProviderTile {source} {onConnect} />
			{/each}
		</div>
	{/if}
</section>

<style>
	.catalog {
		display: flex;
		flex-direction: column;
		gap: 0.625rem;
	}
	header h3 {
		margin: 0;
		font-size: 0.8125rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	header p {
		margin: 0.25rem 0 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
		gap: 0.5rem;
	}

	.error {
		padding: 0.5rem 0.75rem;
		border-radius: 6px;
		background: #fee2e2;
		color: #991b1b;
		font-size: 0.8125rem;
	}
	.muted {
		color: var(--color-foreground-subtle, #9ca3af);
		font-style: italic;
	}
</style>
