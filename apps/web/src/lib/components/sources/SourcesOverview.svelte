<!--
	Sources → Overview. What needs you, then whether data is still arriving.

	The page this replaces opened with three lines explaining what a source is —
	prose you need once, above the two things you came back for. So the running
	state leads, and the definition is gone: Catalog explains sources by showing
	them.

	Broken connections sit above the flow panel because they are the only thing
	here with a verb. A stalled stream is a fact to read; a locked-out credential
	is a job, and only the user can do it.
-->
<script lang="ts">
	import { Page } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import StreamHealthPanel from './StreamHealthPanel.svelte';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { relativeTime } from '$lib/applets/palette';

	const store = sourcesStore;

	$effect(() => {
		void store.load();
	});

	// The OAuth callback 302s back as `?connected=<source_id>` on success and
	// `?source=<id>&error=<reason>` when it didn't finish. Nothing read either,
	// so a round trip through a provider ended in silence whichever way it went.
	// Read once at mount and strip, so a refresh can't replay a stale verdict.
	const connectReturn = (() => {
		if (typeof window === 'undefined') return null;
		const p = new URLSearchParams(window.location.search);
		const connected = p.get('connected');
		const error = p.get('error');
		const source = p.get('source');
		if (!connected && !error) return null;
		for (const k of ['connected', 'error', 'source']) p.delete(k);
		const qs = p.toString();
		window.history.replaceState({}, '', window.location.pathname + (qs ? `?${qs}` : ''));
		return { connected, error, source };
	})();

	let noticeDismissed = $state(false);

	const notice = $derived.by(() => {
		if (!connectReturn || noticeDismissed) return null;
		if (connectReturn.connected) {
			return { ok: true, text: `${store.sourceLabel(connectReturn.connected)} is connected.` };
		}
		const who = connectReturn.source ? store.sourceLabel(connectReturn.source) : 'That source';
		return {
			ok: false,
			text:
				connectReturn.error === 'connect_cancelled'
					? `${who} wasn't connected — the flow was closed before it finished.`
					: `Couldn't finish connecting ${who}. Nothing was connected.`
		};
	});

	const connectedCount = $derived(store.connections.length);
	const sourceCount = $derived(store.bySource.size);

	const lastSeen = $derived.by(() => {
		const stamps = store.connections
			.map((c) => c.lastSeenAt)
			.filter((v): v is string => v !== null);
		if (stamps.length === 0) return null;
		return stamps.reduce((a, b) => (a > b ? a : b));
	});

	async function reconnect(sourceId: string) {
		const source = store.catalogById.get(sourceId);
		if (source) await connectFlow.start(source);
	}

	function openCatalog() {
		windowShellStore.navigate('/sources/catalog', { label: 'Sources · Catalog' });
	}
</script>

<Page
	title="Sources"
	description="Where your data comes from. Anything that needs you appears first, then whether each stream is still arriving."
	maxWidth="wide"
>
	{#snippet actions()}
		<button type="button" class="catalog-btn" onclick={openCatalog}>
			<Icon icon="ri:apps-line" width="15" />
			Catalog
		</button>
	{/snippet}

	{#if !store.loading}
		<p class="tally">
			{#if connectedCount === 0}
				Nothing connected yet.
			{:else}
				{connectedCount}
				{connectedCount === 1 ? 'connection' : 'connections'} across {sourceCount}
				{sourceCount === 1 ? 'source' : 'sources'}.
			{/if}
		</p>
	{/if}

	{#if store.error}
		<div class="error">{store.error}</div>
	{/if}

	{#if connectFlow.error}
		<div class="error">{connectFlow.error}</div>
	{/if}

	{#if notice}
		<div class="notice" class:ok={notice.ok}>
			<Icon icon={notice.ok ? 'ri:check-line' : 'ri:information-line'} width="16" />
			<span>{notice.text}</span>
			<button
				type="button"
				class="notice-x"
				onclick={() => (noticeDismissed = true)}
				aria-label="Dismiss">
				<Icon icon="ri:close-line" width="15" />
			</button>
		</div>
	{/if}

	{#if store.broken.length > 0}
		<ul class="attention">
			{#each store.broken as c (c.id)}
				<li>
					<Icon icon="ri:error-warning-line" width="16" />
					<div class="what">
						<span class="who">{store.sourceLabel(c.sourceId)} · {c.name}</span>
						<span class="why">{c.statusReason ?? 'This connection stopped working.'}</span>
					</div>
					{#if c.kind === 'credential'}
						<button type="button" class="act" onclick={() => void reconnect(c.sourceId)}>
							Reconnect
						</button>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}

	<!-- Connecting a source is only half the story; this is whether it's still
	     delivering. -->
	<StreamHealthPanel />

	{#if !store.loading && connectedCount === 0}
		<div class="empty">
			<Icon icon="ri:plug-line" width="28" />
			<h2>No sources connected yet</h2>
			<p>Pick a provider and it starts filling the record. The catalog has the list.</p>
			<button type="button" class="primary" onclick={openCatalog}>Open the catalog</button>
		</div>
	{:else if !store.loading && store.broken.length === 0}
		<p class="all-well">
			<Icon icon="ri:check-line" width="14" />
			Every connection is healthy{lastSeen ? `, last heard from ${relativeTime(lastSeen)}` : ''}.
		</p>
	{/if}
</Page>

<style>
	.tally {
		margin: 0 0 0.875rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #6b7280);
	}

	.catalog-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.75rem;
		border-radius: 6px;
		border: 1px solid var(--color-border, #d1d5db);
		background: var(--color-background, #fff);
		color: var(--color-foreground, #111827);
		font-size: 0.8125rem;
		font-weight: 500;
		cursor: pointer;
	}
	.catalog-btn:hover {
		background: var(--color-muted, #f3f4f6);
	}

	.error {
		padding: 0.5rem 0.75rem;
		margin-bottom: 0.875rem;
		border-radius: 6px;
		background: var(--color-error-subtle);
		color: color-mix(in srgb, var(--color-error) 75%, #000);
		font-size: 0.8125rem;
	}

	.notice {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
		margin-bottom: 0.875rem;
		border-radius: 6px;
		font-size: 0.8125rem;
		background: var(--color-muted, #f3f4f6);
		color: var(--color-foreground, #111827);
	}
	.notice.ok {
		background: var(--color-success-subtle, #dcfce7);
		color: var(--color-success, #166534);
	}
	.notice span {
		flex: 1;
		min-width: 0;
	}
	.notice-x {
		display: inline-flex;
		border: none;
		background: transparent;
		color: inherit;
		opacity: 0.7;
		cursor: pointer;
		padding: 0;
	}
	.notice-x:hover {
		opacity: 1;
	}

	.attention {
		list-style: none;
		margin: 0 0 0.875rem;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.attention li {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.625rem 0.75rem;
		border-radius: 8px;
		border: 1px solid color-mix(in srgb, var(--color-error) 30%, transparent);
		background: var(--color-error-subtle);
		color: color-mix(in srgb, var(--color-error) 75%, #000);
	}
	.what {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.0625rem;
	}
	.who {
		font-size: 0.8125rem;
		font-weight: 600;
	}
	.why {
		font-size: 0.75rem;
		opacity: 0.85;
	}
	.act {
		flex-shrink: 0;
		padding: 0.3125rem 0.75rem;
		border-radius: 6px;
		border: 1px solid currentColor;
		background: transparent;
		color: inherit;
		font-size: 0.75rem;
		font-weight: 600;
		cursor: pointer;
	}
	.act:hover {
		background: color-mix(in srgb, var(--color-error) 12%, transparent);
	}

	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		padding: 3rem 1rem;
		text-align: center;
		color: var(--color-foreground-muted, #6b7280);
	}
	.empty h2 {
		margin: 0;
		font-size: 1rem;
		font-weight: 600;
		color: var(--color-foreground, #111827);
	}
	.empty p {
		margin: 0;
		font-size: 0.8125rem;
		max-width: 28rem;
	}
	.primary {
		margin-top: 0.5rem;
		padding: 0.4375rem 0.875rem;
		border-radius: 6px;
		border: none;
		background: var(--color-foreground, #111827);
		color: var(--color-background, #fff);
		font-size: 0.8125rem;
		font-weight: 500;
		cursor: pointer;
	}

	.all-well {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		margin: 0.875rem 0 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
</style>
