<!--
	Sources → Activity. What has actually been running.

	`GET /api/runs` — the universal cross-applet run feed — had existed and been
	wired end to end with zero call sites. This is its first consumer. Nothing new
	was needed on the box: the join from a source to its runs is the fan-out
	reconcile already builds, credential_id / device_id on the applet, applet_id
	on the run.

	A run is a record like any other, so this is a UniversalDataGrid like every
	other list here — its filter rail carries source, status and trigger, which a
	hand-rolled row of toggles was doing worse.
-->
<script lang="ts">
	import { Page } from '$lib';
	import UniversalDataGrid, {
		type Column
	} from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { FilterDef } from '$lib/components/datagrid/types';
	import { listRuns, listApplets, type AppletRun, type Applet } from '$lib/api/client';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { relativeTime } from '$lib/applets/palette';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	const store = sourcesStore;

	let runs = $state<AppletRun[]>([]);
	let applets = $state<Applet[]>([]);
	let loading = $state(true);
	let err = $state<string | null>(null);

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

	type Row = {
		id: string;
		applet: string;
		applet_id: string | null;
		source: string;
		status: string;
		trigger: string;
		records: number;
		duration: string;
		started: string;
		started_at: string;
		error: string | null;
	};

	function durationOf(r: AppletRun): string {
		if (!r.completed_at) return r.status === 'running' ? 'running' : '—';
		const ms = new Date(r.completed_at).getTime() - new Date(r.started_at).getTime();
		if (!Number.isFinite(ms) || ms < 0) return '—';
		if (ms < 1000) return `${ms}ms`;
		if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
		return `${Math.round(ms / 60_000)}m`;
	}

	const rows = $derived.by<Row[]>(() =>
		runs.map((r) => {
			const applet = r.applet_id ? appletById.get(r.applet_id) : undefined;
			const sourceId = r.applet_id ? sourceOfApplet.get(r.applet_id) : undefined;
			return {
				id: r.id,
				// A run whose applet is gone keeps its history under a null
				// applet_id — deliberate, it is an audit trail.
				applet: applet?.name ?? (r.applet_id ?? 'deleted applet'),
				applet_id: r.applet_id,
				source: sourceId ? store.sourceLabel(sourceId) : '—',
				status: r.status,
				trigger: r.trigger,
				records: r.records_processed,
				duration: durationOf(r),
				started: relativeTime(r.started_at),
				started_at: r.started_at,
				error: r.error
			};
		})
	);

	const columns: Column<Row>[] = [
		{ key: 'applet', label: 'Applet', icon: 'ri:flashlight-line', width: '28%', minWidth: '160px' },
		{
			key: 'source',
			label: 'Source',
			icon: 'ri:plug-line',
			width: '14%',
			minWidth: '110px',
			groupable: true
		},
		{
			key: 'status',
			label: 'Status',
			icon: 'ri:circle-line',
			width: '12%',
			minWidth: '100px',
			format: 'badge',
			badgeColors: {
				success: 'badge-success',
				error: 'badge-error',
				running: 'badge-warning',
				skipped: 'badge-muted',
				cancelled: 'badge-muted'
			}
		},
		{
			key: 'trigger',
			label: 'Trigger',
			icon: 'ri:play-line',
			width: '10%',
			minWidth: '90px',
			groupable: true,
			hideOnMobile: true
		},
		{
			key: 'records',
			label: 'Records',
			icon: 'ri:database-2-line',
			width: '10%',
			minWidth: '90px',
			format: 'number',
			hideOnMobile: true
		},
		{
			key: 'duration',
			label: 'Took',
			icon: 'ri:timer-line',
			width: '10%',
			minWidth: '80px',
			hideOnMobile: true
		},
		{
			key: 'started',
			label: 'Started',
			icon: 'ri:time-line',
			width: '16%',
			minWidth: '110px',
			// Sort on the timestamp, not on "3 minutes ago".
			getValue: (r) => r.started
		}
	];

	const filters: FilterDef<Row>[] = [
		{
			id: 'status',
			kind: 'multi',
			label: 'Status',
			options: [
				{ value: 'error', label: 'Failed' },
				{ value: 'success', label: 'Succeeded' },
				{ value: 'running', label: 'Running' },
				{ value: 'skipped', label: 'Skipped' }
			],
			predicate: (r, v) => Array.isArray(v) && v.includes(r.status)
		},
		{
			id: 'source',
			kind: 'enum',
			label: 'From',
			options: [
				{ value: 'sources', label: 'Sources only' },
				{ value: 'all', label: 'Everything' }
			],
			predicate: (r, v) => v !== 'sources' || r.source !== '—'
		}
	];

	function open(row: Row) {
		if (row.applet_id) {
			windowShellStore.navigate(`/applet/${row.applet_id}`, { label: row.applet });
		}
	}
</script>

<Page
	title="Activity"
	description="Every applet run on this box, newest first. Filter to just your sources, or to what failed."
	maxWidth="wide"
>
	{#if err}
		<div class="error">{err}</div>
	{/if}

	<UniversalDataGrid
		items={rows}
		{columns}
		{filters}
		entityType="applet-run"
		{loading}
		error={null}
		emptyIcon="ri:history-line"
		emptyMessage="No runs recorded yet"
		loadingMessage="Loading runs…"
		searchPlaceholder="Search runs…"
		defaultViewMode="table"
		onItemClick={open}
		onRefresh={load}
		onRetry={load}
	/>
</Page>

<style>
	.error {
		padding: 0.5rem 0.75rem;
		margin-bottom: 0.75rem;
		border-radius: 6px;
		background: var(--color-error-subtle);
		color: color-mix(in srgb, var(--color-error) 75%, #000);
		font-size: 0.8125rem;
	}
</style>
