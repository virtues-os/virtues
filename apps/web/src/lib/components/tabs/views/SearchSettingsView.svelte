<script lang="ts">
	/**
	 * Settings → Search: where search runs, whether it's answering, and how much
	 * of the record it covers. Read-only on purpose. Moving search to another
	 * server re-checks the model and can rebuild the index, so it lives in one
	 * command (`virtues configure-inference --embed-url`, or `--recommended` to
	 * come back), which this page names.
	 *
	 * When the installer found a GPU or NPU that search isn't using, the page
	 * keeps pointing at its guide until the owner dismisses it.
	 */
	import { onMount } from 'svelte';
	import { Button, Page } from '$lib';
	import { getSearchStatus, type SearchStatus } from '$lib/api/client';
	import { openExternal } from '$lib/tauri/bridge';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';

	const GUIDE = 'https://virtues.com/docs/setup/accelerators';
	const DISMISS_KEY = 'virtues.search.acceleratorDismissed';

	let status = $state<SearchStatus | null>(null);
	let error = $state<string | null>(null);
	let dismissed = $state<string | null>(null);

	async function load() {
		error = null;
		try {
			status = await getSearchStatus();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}

	onMount(() => {
		try {
			dismissed = localStorage.getItem(DISMISS_KEY);
		} catch {
			dismissed = null;
		}
		load();
	});

	function dismiss() {
		if (!status?.accelerator) return;
		dismissed = status.accelerator;
		try {
			localStorage.setItem(DISMISS_KEY, status.accelerator);
		} catch {
			// Private mode: the card simply comes back next visit.
		}
	}

	const showAccelerator = $derived(!!status?.accelerator && dismissed !== status.accelerator);

	const changePercent = $derived.by(() => {
		const c = status?.model_change;
		if (!c || c.total === 0) return 0;
		return Math.min(99, Math.floor((c.done / c.total) * 100));
	});

	function host(url: string): string {
		try {
			return new URL(url).host;
		} catch {
			return url;
		}
	}

	const runsOn = $derived.by(() => {
		if (!status) return '';
		switch (status.mode) {
			case 'dragon':
				return "This server's NPU";
			case 'bundled':
				return "This server's CPU";
			case 'manual':
				return `Your server at ${host(status.embed_url)}`;
			default:
				return status.embed_url;
		}
	});
</script>

<Page title="Search" description="Where search runs, and how much of your record it covers." maxWidth="wide">
	{#if error}
		<p class="error">
			Couldn't load search status: {error}.
			<button type="button" class="link" onclick={load}>Try again</button>
		</p>
	{:else if !status}
		<p class="dim">Loading…</p>
	{:else}
		{#if showAccelerator}
			<section class="accelerator">
				<h3>{status.accelerator} found</h3>
				<p>
					Search runs on this server's CPU. On the {status.accelerator}, every search and your
					first index are much faster. Set it up with the guide - about ten minutes - then point
					search at it by running this on the server:
				</p>
				<pre><code>virtues configure-inference --embed-url http://127.0.0.1:8080</code></pre>
				<div class="actions">
					<Button onclick={() => openExternal(status?.accelerator_guide ?? GUIDE)}>Open the setup guide</Button>
					<Button variant="ghost" onclick={dismiss}>Not now</Button>
				</div>
			</section>
		{/if}

		<dl class="ledger">
			<dt>Runs on</dt>
			<dd>{runsOn}</dd>

			<dt>Model</dt>
			<dd>
				{status.index_model ?? 'Nothing indexed yet'}
				{#if status.index_dims}<span class="dim"> · {status.index_dims} dimensions</span>{/if}
				{#if status.model_change}
					<span class="note">
						Moving to {status.model_change.model}: {changePercent}% of the index is rebuilt. Search
						keeps using the current model until the rebuild finishes.
					</span>
				{/if}
			</dd>

			<dt>Status</dt>
			<dd>
				{#if status.reachable}
					Answering in {status.probe_ms} ms
				{:else}
					<span class="bad">Not answering</span>
					<span class="note">
						New searches can't run until the server at {status.embed_url} answers. Check that it's
						running, then reload this page.
					</span>
				{/if}
			</dd>

			<dt>Searchable</dt>
			<dd>
				{status.records_searchable.toLocaleString()} records
				{#if status.last_indexed_at}
					<span class="dim"> · last indexed {formatRelativeTimestamp(status.last_indexed_at)}</span>
				{/if}
			</dd>

			<dt>Reranker</dt>
			<dd>
				{status.rerank_on ? 'On' : 'Off'}
				{#if !status.rerank_on}
					<span class="note">
						In our tests, the small rerankers ranked results worse than search without them.
					</span>
				{/if}
			</dd>
		</dl>

		{#if status.mode !== 'manual' && status.mode !== 'dragon'}
			<section class="own">
				<h3>Run search on your own server</h3>
				<p>
					Start an embedding server on a GPU, an NPU, or another machine, then run this on the
					server. The command keeps your index when the new server runs the same model.
				</p>
				<pre><code>virtues configure-inference --embed-url http://127.0.0.1:8080</code></pre>
				<button type="button" class="link" onclick={() => openExternal(GUIDE)}>
					Read the guide for GPUs and NPUs
				</button>
			</section>
		{:else if status.mode === 'manual'}
			<section class="own">
				<h3>Go back to the recommended setup</h3>
				<p>
					Your server runs the model you chose, and updates leave it alone. To have Virtues run
					its recommended model on this server's CPU and keep it current, run this on the server.
					Search keeps using your server until the index is rebuilt for the new model.
				</p>
				<pre><code>sudo virtues configure-inference --recommended</code></pre>
			</section>
		{/if}
	{/if}
</Page>

<style>
	.accelerator {
		padding: 16px 20px;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: var(--color-surface-elevated);
		margin-bottom: 24px;
	}

	.accelerator p,
	.own p {
		margin: 8px 0 12px;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		max-width: 62ch;
	}

	h3 {
		font-size: 14px;
		font-weight: 600;
		margin: 0;
	}

	pre {
		margin: 0 0 12px;
		padding: 8px 12px;
		border-radius: 6px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		overflow-x: auto;
	}

	code {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 12px;
	}

	.actions {
		display: flex;
		gap: 8px;
	}

	.ledger {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 12px 16px;
		align-items: baseline;
		margin: 0;
		font-size: 13px;
	}

	dt {
		color: var(--color-foreground-subtle);
	}

	dd {
		margin: 0;
	}

	.note {
		display: block;
		color: var(--color-foreground-muted);
		font-size: 12px;
		line-height: 1.5;
		max-width: 60ch;
	}

	.dim {
		color: var(--color-foreground-subtle);
	}

	.bad {
		color: var(--color-error);
	}

	.own {
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--color-border);
	}

	.link {
		background: none;
		border: none;
		padding: 0;
		color: var(--color-primary);
		font: inherit;
		font-size: 13px;
		cursor: pointer;
		text-decoration: underline;
	}

	.error {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
</style>
