<!--
	What AI has cost → Every call: each AI call your server made, newest
	first, with what it was for, the model and what it cost. The page's month,
	chart and kinds above are these calls added up (BillingView).

	Served a page at a time from your server's call log (`/telemetry/ai-calls`);
	Show more adds the next page, and the search asks the server, so a call
	from months ago is reachable.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import Button from '$lib/components/Button.svelte';
	import SearchField from '$lib/components/SearchField.svelte';
	import { formatMicrosPrecise } from '$lib/utils/currency';
	import { featureLabel } from '$lib/utils/aiFeatures';
	import { whenAt } from '$lib/settings/changes';
	import { getAiCallsPage, ApiError, type AiCallRow } from '$lib/api/client';
	import { modelSlots } from '$lib/stores/modelSlots.svelte';

	const PAGE = 25;

	let calls = $state<AiCallRow[]>([]);
	let total = $state(0);
	let search = $state('');
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Model names come from the catalog.
	onMount(() => {
		if (!modelSlots.loaded && !modelSlots.loading) void modelSlots.load();
	});

	/** Load from the start (`more` false) or the next page. A newer search wins. */
	let asked = 0;
	async function load(more: boolean) {
		const ask = ++asked;
		loading = true;
		error = null;
		try {
			const page = await getAiCallsPage({ offset: more ? calls.length : 0, limit: PAGE, search: search.trim() || undefined });
			if (ask !== asked) return;
			calls = more ? [...calls, ...page.items] : page.items;
			total = page.total;
		} catch (e) {
			if (ask !== asked) return;
			error = e instanceof ApiError ? e.message : "Your server couldn't read its call log.";
		} finally {
			if (ask === asked) loading = false;
		}
	}

	// A moment after typing stops, the list starts over for the new search.
	$effect(() => {
		void search;
		const timer = setTimeout(() => void load(false), 250);
		return () => clearTimeout(timer);
	});

	/**
	 * What a call cost. Only the Virtues gateway reports a price, so a call on
	 * your own key has none to show: "$0.00" there would read as free.
	 */
	function cost(c: AiCallRow): string {
		return c.route === 'byo' ? 'on your key' : formatMicrosPrecise(c.cost_micros);
	}
</script>

<div class="search">
	<SearchField bind:value={search} placeholder="Search by what it was for or the model" label="Search calls" />
</div>

{#if error}
	<div class="failed" role="alert">
		<p>{error}</p>
		<Button variant="secondary" onclick={() => load(false)}>Try again</Button>
	</div>
{:else if !loading && calls.length === 0}
	<p class="quiet">{search.trim() ? `No call matches “${search.trim()}”.` : 'No AI calls yet.'}</p>
{:else}
	<ul class="calls" aria-busy={loading}>
		{#each calls as c (c.id)}
			<li>
				<span class="when">{whenAt(c.created_at)}</span>
				<span class="what">{c.feature ? featureLabel(c.feature) : 'Other'}</span>
				<span class="model">{c.route === 'byo' ? 'Your own key' : c.model ? modelSlots.nameOf(c.model) : '—'}</span>
				<span class="cost" class:muted={c.route === 'byo'}>{cost(c)}</span>
			</li>
		{/each}
	</ul>
	{#if calls.length < total}
		<div class="more">
			<Button variant="secondary" loading={loading} onclick={() => load(true)}>Show more</Button>
		</div>
	{/if}
{/if}

<style>
	.search {
		display: flex;
	}

	.calls {
		margin: 8px 0 0;
		padding: 0;
		list-style: none;
	}

	.calls li {
		display: grid;
		grid-template-columns: 136px minmax(0, 1fr) minmax(0, 200px) 88px;
		align-items: baseline;
		gap: 16px;
		padding: 10px 0 0;
		min-height: 30px;
	}

	.when {
		font-size: 13px;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	.what {
		font-size: 15px;
		color: var(--color-foreground);
	}

	.model {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.cost {
		text-align: right;
		font-size: 14px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground);
	}

	.cost.muted {
		font-size: 13px;
		color: var(--color-foreground-subtle);
	}

	.more {
		margin-top: 16px;
	}

	.quiet {
		margin: 16px 0 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.failed {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
		margin-top: 16px;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.failed p {
		margin: 0;
	}

	@media (max-width: 640px) {
		.calls li {
			grid-template-columns: minmax(0, 1fr) 88px;
		}

		.when {
			grid-column: 1 / -1;
		}

		.model {
			display: none;
		}
	}
</style>
