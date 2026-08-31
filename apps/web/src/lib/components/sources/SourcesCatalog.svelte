<!--
	Sources → Catalog. Every source, with what's connected to it underneath.

	This replaces a flat grid of credentials sitting under a "+ Connect" popover.
	Two things were wrong with that. The catalog was transient — the roster of
	what you *could* connect vanished when the popover closed, so the page could
	only ever answer "what have I already done". And the grid listed credentials,
	which meant iOS and Mac — the two tiles at the top of that popover — could
	never appear in it, because device sources pair into `app_device` and never
	mint a credential. A paired iPhone showed up only in Settings → Devices.

	So the row is a *source*, not a connection, and every source has one whether
	or not you've connected it — "what could I plug in" is half of what this page
	is for. Opening a row goes to that source's page, where its connections live
	and where both kinds are treated alike (see lib/stores/sources.svelte.ts).
-->
<script lang="ts">
	import { Page } from '$lib';
	import UniversalDataGrid, {
		type Column
	} from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { sourcesStore, type Connection } from '$lib/stores/sources.svelte';
	import { getStreamHealth, type StreamHealth } from '$lib/api/client';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { relativeTime } from '$lib/applets/palette';
	import { isMacOS, isTauri, thisComputerLabel } from '$lib/utils/platform';
	import { domainLabel } from '$lib/sources/domains';
	import type { SourceCatalogItem } from '$lib/api/client';

	const store = sourcesStore;

	let streams = $state<StreamHealth[]>([]);

	$effect(() => {
		void store.load();
		// Failure here costs the footnote, not the table.
		getStreamHealth()
			.then((r) => (streams = r))
			.catch(() => (streams = []));
	});

	/**
	 * Ontologies nothing in the catalog can produce. The coverage question the
	 * Overview deliberately no longer answers: its axis is "is what I have
	 * working", and this one is "what could I have at all". Stated once, at the
	 * bottom, because it is a limit of ours rather than something the reader did
	 * wrong — and with no count, for the same reason the Overview carries no
	 * score.
	 */
	// Only claim this when the box demonstrably serves provider data. On a build
	// that predates it every stream looks unprovided, and the footnote would
	// confidently list streams an already-paired iPhone writes — the exact class
	// of unsubstantiated claim this whole pass exists to remove.
	const providerDataAvailable = $derived(streams.some((s) => (s.provided_by ?? []).length > 0));
	const uncovered = $derived(
		providerDataAvailable
			? streams
					.filter((s) => (s.provided_by ?? []).length === 0 && s.total === 0)
					.map((s) => s.display_name)
			: []
	);

	/** One catalog row: a source plus whatever is connected to it. */
	type Row = {
		id: string;
		name: string;
		description: string;
		icon: string;
		kind: string;
		connections: Connection[];
		connected: number;
		state: string;
		last_activity: string;
		applets: number;
		/** Life-domains this source can fill, e.g. "Health · Location". */
		provides: string;
		/** Null when the connection's source is no longer in the catalog. */
		source: SourceCatalogItem | null;
	};

	const AUTH_LABEL: Record<string, string> = {
		self_issued_bearer: 'Device',
		via_proxy: 'Account',
		api_key: 'Key'
	};

	/**
	 * A connection whose source is no longer in the catalog. Possible now that
	 * a package can contribute sources and therefore remove them — and nothing
	 * reconciles credentials against the catalog, so the row survives with a
	 * `source_id` that resolves to nothing. Without a home here it would be
	 * invisible and un-revokable while its applets kept running.
	 */
	let query = $state('');

	const orphanRows = $derived.by<Row[]>(() => {
		const known = new Set(store.catalog.map((s) => s.id));
		const out: Row[] = [];
		for (const [sourceId, connections] of store.bySource) {
			if (known.has(sourceId)) continue;
			out.push({
				id: sourceId,
				name: sourceId,
				description:
					'This connection’s source is no longer installed. Nothing will run for it; disconnect it if you no longer need it.',
				icon: 'ri:question-line',
				kind: 'unknown',
				connections,
				connected: connections.length,
				state: 'needs attention',
				last_activity: '—',
				applets: connections.reduce((n, c) => n + (c.appletCount ?? 0), 0),
				provides: '—',
				source: null
			});
		}
		return out;
	});

	const catalogRows = $derived.by<Row[]>(() =>
		store.catalog.map((source) => {
			const connections = store.bySource.get(source.id) ?? [];
			const stamps = connections
				.map((c) => c.lastSeenAt)
				.filter((v): v is string => v !== null);
			const newest = stamps.length ? stamps.reduce((a, b) => (a > b ? a : b)) : null;
			return {
				id: source.id,
				name: source.name,
				description: source.description ?? '',
				icon: source.icon ?? 'ri:plug-line',
				kind: AUTH_LABEL[source.auth_kind] ?? source.auth_kind,
				connections,
				connected: connections.length,
				state: connections.some((c) => c.broken)
					? 'needs attention'
					: connections.length > 0
						? 'connected'
						: 'not connected',
				last_activity: newest ? relativeTime(newest) : '—',
				applets: connections.reduce((n, c) => n + (c.appletCount ?? 0), 0),
				provides: (source.domains ?? []).map(domainLabel).join(' · ') || '—',
				source
			};
		})
	);

	// Orphans first: they are the only rows here that are actively wrong.
	const rows = $derived.by<Row[]>(() => {
		const q = query.trim().toLowerCase();
		return [...orphanRows, ...catalogRows]
			.filter((r) => !q || r.name.toLowerCase().includes(q))
			.sort((a, b) => {
				const rank = (r: Row) => (r.source === null ? 0 : r.connections.length > 0 ? 1 : 2);
				return rank(a) - rank(b) || a.name.localeCompare(b.name);
			});
	});

	const columns: Column<Row>[] = [
		{ key: 'name', label: 'Source', icon: 'ri:plug-line', width: '20%', minWidth: '140px' },
		{
			key: 'provides',
			label: 'Provides',
			icon: 'ri:stack-line',
			width: '22%',
			minWidth: '150px'
		},
		{
			key: 'kind',
			label: 'Connects by',
			icon: 'ri:key-2-line',
			width: '12%',
			minWidth: '100px',
			groupable: true,
			hideOnMobile: true
		},
		{
			key: 'state',
			label: 'Status',
			icon: 'ri:circle-line',
			width: '18%',
			minWidth: '120px',
			format: 'badge',
			badgeColors: {
				connected: 'badge-success',
				'needs attention': 'badge-error',
				'not connected': 'badge-muted'
			}
		},
		{
			key: 'connected',
			label: 'Connections',
			icon: 'ri:links-line',
			width: '14%',
			minWidth: '110px',
			format: 'number'
		},
		{
			key: 'last_activity',
			label: 'Last activity',
			icon: 'ri:time-line',
			width: '16%',
			minWidth: '120px',
			hideOnMobile: true
		},
		{
			key: 'applets',
			label: 'Applets',
			icon: 'ri:flashlight-line',
			width: '12%',
			minWidth: '90px',
			format: 'number',
			hideOnMobile: true
		}
	];

	/**
	 * Device sources are the one case where "connect" depends on where you're
	 * reading. Inside the Mac app the Mac is already here — it needs turning on,
	 * not pairing, and DeviceView owns that. Handing the user a six-digit code
	 * to carry to the app they are currently *using* is the narrative break this
	 * avoids.
	 */
	function isThisDevice(source: SourceCatalogItem): boolean {
		return isTauri && isMacOS && source.id === 'mac';
	}

	function connectLabel(row: Row): string {
		if (!row.source) return '';
		if (isThisDevice(row.source)) return `Set up ${thisComputerLabel}`;
		// A Mac is never paired by a code — its collector is switched on inside
		// that Mac's own app. Saying "Pair a device" here promised a flow that
		// paired the viewer and collected nothing.
		if (row.source.id === 'mac') return 'How to set up';
		if (row.source.auth_kind === 'self_issued_bearer') return 'Pair a device';
		return row.connected > 0 ? 'Connect another' : 'Connect';
	}

	async function connect(source: SourceCatalogItem) {
		if (isThisDevice(source)) {
			// This Mac stopped being a top-level Settings section on 2026-08-17 —
			// it is a device page now, and `this` is the stable name for the
			// machine you're on (no device id to look up first).
			windowShellStore.navigate('/virtues/devices/this', { label: 'This Mac' });
			return;
		}
		await connectFlow.start(source);
	}

	function openSource(row: Row) {
		windowShellStore.navigate(`/sources/${row.id}`, { label: row.name });
	}
</script>

<Page
	title="Catalog"
	description="Everything Virtues can draw from, and what you have connected to each."
	maxWidth="wide"
>
	{#if store.error}
		<div class="error">{store.error}</div>
	{/if}
	{#if connectFlow.error}
		<div class="error">{connectFlow.error}</div>
	{/if}

	<UniversalDataGrid
		items={rows}
		{columns}
		entityType="source"
		loading={store.loading}
		error={null}
		emptyIcon="ri:plug-line"
		emptyMessage="No sources in the catalog"
		loadingMessage="Loading sources…"
		searchPlaceholder="Search sources…"
		defaultViewMode="table"
		onItemClick={openSource}
		onRetry={() => store.load()}
	>
		{#snippet rowActions(row: Row)}
			{#if row.source}
				<button type="button" class="connect" onclick={() => void connect(row.source!)}>
					{connectLabel(row)}
				</button>
			{/if}
		{/snippet}

	</UniversalDataGrid>

	{#if uncovered.length > 0}
		<p class="uncovered">
			No source yet for {uncovered.join(', ')}. Virtues can hold these, but
			nothing installed writes them.
		</p>
	{/if}
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

	.uncovered {
		margin: 1rem 0 0;
		max-width: 44rem;
		font-size: 0.8125rem;
		line-height: 1.55;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.connect {
		padding: 0.25rem 0.625rem;
		border-radius: 6px;
		border: 1px solid var(--color-border, #d1d5db);
		background: var(--color-background, #fff);
		color: var(--color-foreground, #111827);
		font-size: 0.75rem;
		font-weight: 500;
		white-space: nowrap;
		cursor: pointer;
	}
	.connect:hover {
		background: var(--color-muted, #f3f4f6);
	}

</style>
