<!--
	Sources → Activity. What has actually been running.

	`GET /api/runs` — the universal cross-applet run feed — has existed and been
	wired end to end for a while, and `listRuns()` had zero call sites. This is
	its first consumer. Nothing new was needed on the box: the join from a source
	to its runs is the fan-out reconcile already builds, credential_id / device_id
	on the applet, applet_id on the run.

	Scoped to source applets by default (origin === 'source'), because that is
	the question this room asks. The toggle widens it to every applet rather than
	sending you to Developer → Telemetry for the aggregate.
-->
<script lang="ts">
	import { Page } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import { listRuns, listApplets, type AppletRun, type Applet } from '$lib/api/client';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { relativeTime } from '$lib/applets/palette';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	const store = sourcesStore;

	let runs = $state<AppletRun[]>([]);
	let applets = $state<Applet[]>([]);
	let loading = $state(true);
	let err = $state<string | null>(null);
	let sourcesOnly = $state(true);
	let statusFilter = $state<'all' | 'error' | 'success'>('all');

	async function load() {
		loading = true;
		err = null;
		try {
			const [r, a] = await Promise.all([listRuns({ limit: 200 }), listApplets()]);
			runs = r;
			applets = a;
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
		void store.load();
	});

	const appletById = $derived(new Map(applets.map((a) => [a.id, a])));

	/**
	 * Which source an applet belongs to. The applet carries the anchor
	 * (credential_id or device_id); the connection list carries the source. One
	 * hop, and it is the same hop the fan-out made in the other direction.
	 */
	const sourceOfApplet = $derived.by(() => {
		const byConnection = new Map(store.connections.map((c) => [c.id, c.sourceId]));
		const m = new Map<string, string>();
		for (const a of applets) {
			const anchor = a.credential_id ?? a.device_id;
			if (!anchor) continue;
			const sourceId = byConnection.get(anchor);
			if (sourceId) m.set(a.id, sourceId);
		}
		return m;
	});

	const rows = $derived.by(() =>
		runs
			.map((r) => {
				const applet = r.applet_id ? appletById.get(r.applet_id) : undefined;
				const sourceId = r.applet_id ? sourceOfApplet.get(r.applet_id) : undefined;
				return {
					run: r,
					applet,
					// A run whose applet is gone keeps its history under a null
					// applet_id — deliberate, it's an audit trail.
					name: applet?.name ?? (r.applet_id ? r.applet_id : 'deleted applet'),
					sourceLabel: sourceId ? store.sourceLabel(sourceId) : null
				};
			})
			.filter((row) => (sourcesOnly ? row.sourceLabel !== null : true))
			.filter((row) => statusFilter === 'all' || row.run.status === statusFilter)
	);

	function openApplet(appletId: string | null) {
		if (appletId) windowShellStore.navigate(`/applet/${appletId}`, { label: 'Applet' });
	}

	function duration(r: AppletRun): string {
		if (!r.completed_at) return r.status === 'running' ? 'running' : '—';
		const ms = new Date(r.completed_at).getTime() - new Date(r.started_at).getTime();
		if (!Number.isFinite(ms) || ms < 0) return '—';
		if (ms < 1000) return `${ms}ms`;
		if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
		return `${Math.round(ms / 60_000)}m`;
	}
</script>

<Page
	title="Activity"
	description="Every run your sources have made, newest first — the same run history the box keeps for every applet, scoped to what your sources did."
	maxWidth="wide"
>
	{#snippet actions()}
		<div class="filters">
			<button type="button" class:on={sourcesOnly} onclick={() => (sourcesOnly = !sourcesOnly)}>
				{sourcesOnly ? 'Sources only' : 'All applets'}
			</button>
			<button
				type="button"
				class:on={statusFilter === 'error'}
				onclick={() => (statusFilter = statusFilter === 'error' ? 'all' : 'error')}
			>
				Failures
			</button>
			<button type="button" class="icon" onclick={() => void load()} aria-label="Refresh">
				<Icon icon="ri:refresh-line" width="15" />
			</button>
		</div>
	{/snippet}

	{#if err}
		<div class="error">{err}</div>
	{/if}

	{#if loading}
		<p class="muted">Loading runs…</p>
	{:else if rows.length === 0}
		<p class="muted">
			{sourcesOnly
				? 'No runs from source applets yet. Connect a source and its first sync will land here.'
				: 'No runs recorded yet.'}
		</p>
	{:else}
		<ul class="runs">
			{#each rows as row (row.run.id)}
				<li class="run">
					<span class="dot {row.run.status}" aria-hidden="true"></span>
					<button type="button" class="who" onclick={() => openApplet(row.run.applet_id)}>
						{row.name}
					</button>
					<span class="src">{row.sourceLabel ?? ''}</span>
					<span class="trigger">{row.run.trigger}</span>
					<span class="records">
						{row.run.records_processed > 0 ? `${row.run.records_processed} records` : ''}
					</span>
					<span class="dur">{duration(row.run)}</span>
					<span class="when">{relativeTime(row.run.started_at)}</span>
				</li>
				{#if row.run.error}
					<li class="run-error">{row.run.error}</li>
				{/if}
			{/each}
		</ul>
	{/if}
</Page>

<style>
	.filters {
		display: flex;
		gap: 0.375rem;
		flex-shrink: 0;
	}
	.filters button {
		padding: 0.3125rem 0.625rem;
		border-radius: 6px;
		border: 1px solid var(--color-border, #d1d5db);
		background: var(--color-background, #fff);
		color: var(--color-foreground-muted, #6b7280);
		font-size: 0.75rem;
		cursor: pointer;
	}
	.filters button.on {
		background: var(--color-muted, #f3f4f6);
		color: var(--color-foreground, #111827);
		font-weight: 500;
	}
	.filters .icon {
		display: inline-flex;
		align-items: center;
	}

	.error {
		padding: 0.5rem 0.75rem;
		border-radius: 6px;
		background: var(--color-error-subtle);
		color: color-mix(in srgb, var(--color-error) 75%, #000);
		font-size: 0.8125rem;
	}
	.muted {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #6b7280);
	}

	.runs {
		list-style: none;
		margin: 0;
		padding: 0;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 10px;
		overflow: hidden;
	}
	.run {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.4375rem 0.875rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.run + .run,
	.run-error + .run {
		border-top: 1px solid var(--color-border, #e5e7eb);
	}

	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		flex-shrink: 0;
		background: var(--color-foreground-subtle, #9ca3af);
	}
	.dot.success {
		background: var(--color-success, #16a34a);
	}
	.dot.error {
		background: var(--color-error);
	}
	.dot.running {
		background: var(--color-warning, #d97706);
	}

	.who {
		flex: 1;
		min-width: 0;
		text-align: left;
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--color-foreground, #111827);
		cursor: pointer;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.who:hover {
		text-decoration: underline;
	}

	.src {
		flex-shrink: 0;
		width: 7rem;
	}
	.trigger {
		flex-shrink: 0;
		width: 4.5rem;
	}
	.records {
		flex-shrink: 0;
		width: 6.5rem;
	}
	.dur {
		flex-shrink: 0;
		width: 3.5rem;
		text-align: right;
	}
	.when {
		flex-shrink: 0;
		width: 7rem;
		text-align: right;
	}

	.run-error {
		padding: 0 0.875rem 0.5rem 2.125rem;
		font-size: 0.6875rem;
		font-family: var(--font-mono, ui-monospace, monospace);
		color: color-mix(in srgb, var(--color-error) 80%, #000);
		white-space: pre-wrap;
		word-break: break-word;
	}

	@media (max-width: 900px) {
		.src,
		.trigger,
		.records,
		.dur {
			display: none;
		}
	}
</style>
