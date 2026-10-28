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

	Services the assistant can act in (other services' MCP servers, see
	agents/plan/mcp-plan.md) are rows here too, because connecting one is the
	same act as connecting a source: you sign in to a service. The "What it
	does" column tells the two apart. A source syncs your data to your server.
	A connection syncs nothing; the assistant uses its tools when you ask in
	chat. One search covers both: what you type also searches the public MCP
	Registry, and its matches are listed under the table.
-->
<script lang="ts">
	import { Page } from '$lib';
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import UniversalDataGrid, {
		type Column
	} from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { sourcesStore, type Connection } from '$lib/stores/sources.svelte';
	import { apiGet, apiSend, getStreamHealth, type StreamHealth } from '$lib/api/client';
	import { canSignInSheet, openSignIn } from '$lib/tauri/bridge';
	import { reloadOnReturn } from './connectDispatch';
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
		void loadConnections();
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

	/** A service the assistant can act in, as `/api/connections` returns it. */
	type Connected = { id: string; name: string; about: string; url: string; registry: string | null };
	type Listing = { registry: string; title: string; about: string; url: string; is_connected: boolean };

	const SYNCS = 'Syncs your data';
	const ACTS = 'The assistant can act here';

	let connected = $state<Connected[]>([]);
	let featured = $state<Listing[]>([]);
	let registryError = $state<string | null>(null);
	let connectionsError = $state<string | null>(null);
	/** The key of the connect or disconnect in flight, so its button spins. */
	let busy = $state<string | null>(null);
	let results = $state<Listing[] | null>(null);
	let searching = $state(false);
	let address = $state('');

	const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

	async function loadConnections() {
		try {
			const list = await apiGet<{
				connected: Connected[];
				featured: Listing[];
				registry_error: string | null;
			}>('/connections');
			connected = list.connected;
			featured = list.featured;
			registryError = list.registry_error;
			connectionsError = null;
		} catch (e) {
			connectionsError = message(e);
		}
	}

	/** One catalog row: a source, or a service the assistant can act in. */
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
		/** "Syncs your data" or "The assistant can act here". */
		does: string;
		/** Null for a service the assistant acts in, and for a connection
		 *  whose source is no longer in the catalog. */
		source: SourceCatalogItem | null;
		/** Set on a service the assistant acts in. `id` is null until it is
		 *  connected. */
		service: { id: string | null; registry: string | null } | null;
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
				icon: 'mynaui:question-circle',
				kind: 'unknown',
				connections,
				connected: connections.length,
				state: 'needs attention',
				last_activity: '—',
				applets: connections.reduce((n, c) => n + (c.appletCount ?? 0), 0),
				provides: '—',
				does: SYNCS,
				source: null,
				service: null
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
				icon: source.icon ?? 'mynaui:power',
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
				does: SYNCS,
				source,
				service: null
			};
		})
	);

	function serviceRow(
		id: string,
		name: string,
		about: string,
		service: { id: string | null; registry: string | null }
	): Row {
		const on = service.id !== null;
		return {
			id,
			name,
			description: about,
			icon: 'mynaui:link',
			kind: AUTH_LABEL.via_proxy,
			connections: [],
			connected: on ? 1 : 0,
			state: on ? 'connected' : 'not connected',
			last_activity: '—',
			applets: 0,
			provides: '—',
			does: ACTS,
			source: null,
			service
		};
	}

	// What you have connected, then the featured services you haven't.
	const serviceRows = $derived.by<Row[]>(() => {
		const out = connected.map((c) =>
			serviceRow(c.id, c.name, c.about, { id: c.id, registry: c.registry })
		);
		const have = new Set(connected.map((c) => c.registry));
		for (const l of featured) {
			if (have.has(l.registry)) continue;
			out.push(serviceRow(`registry:${l.registry}`, l.title, l.about, { id: null, registry: l.registry }));
		}
		return out;
	});

	// Registry matches the table does not already list.
	const extraResults = $derived.by(() => {
		if (!results) return null;
		const listed = new Set(serviceRows.map((r) => r.service?.registry));
		return results.filter((l) => !listed.has(l.registry));
	});

	// Orphans first: they are the only rows here that are actively wrong. Then
	// what you have connected, then the rest.
	const rows = $derived.by<Row[]>(() =>
		[...orphanRows, ...catalogRows, ...serviceRows].sort((a, b) => {
			const rank = (r: Row) =>
				r.source === null && r.service === null ? 0 : r.connected > 0 ? 1 : 2;
			return rank(a) - rank(b) || a.name.localeCompare(b.name);
		})
	);

	// The registry search runs a moment after typing stops, so each keystroke
	// is not a request to the registry.
	let timer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		const q = query.trim();
		clearTimeout(timer);
		if (!q) {
			results = null;
			return;
		}
		timer = setTimeout(async () => {
			searching = true;
			try {
				const found = await apiGet<{ results: Listing[] }>('/connections/search', { q });
				if (query.trim() === q) results = found.results;
			} catch (e) {
				connectionsError = message(e);
			} finally {
				searching = false;
			}
		}, 300);
	});

	const columns: Column<Row>[] = [
		{ key: 'name', label: 'Service', icon: 'mynaui:power', width: '14%', minWidth: '120px' },
		{
			key: 'does',
			label: 'What it does',
			icon: 'mynaui:info-circle',
			width: '20%',
			minWidth: '200px',
			groupable: true
		},
		{
			key: 'provides',
			label: 'Provides',
			icon: 'mynaui:layers-three',
			width: '18%',
			minWidth: '150px'
		},
		{
			key: 'kind',
			label: 'Connects by',
			icon: 'mynaui:key',
			width: '12%',
			minWidth: '100px',
			groupable: true,
			hideOnMobile: true
		},
		{
			key: 'state',
			label: 'Status',
			icon: 'mynaui:circle',
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
			icon: 'mynaui:link',
			width: '14%',
			minWidth: '110px',
			format: 'number'
		},
		{
			key: 'last_activity',
			label: 'Last activity',
			icon: 'mynaui:clock-circle',
			width: '16%',
			minWidth: '120px',
			hideOnMobile: true
		},
		{
			key: 'applets',
			label: 'Applets',
			icon: 'mynaui:zap',
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
		// A service the assistant acts in has no page; its row carries its one
		// action.
		if (row.service) return;
		windowShellStore.navigate(`/sources/${row.id}`, { label: row.name });
	}

	/**
	 * After a sign-in the service sends the browser to the OAuth proxy, and
	 * your server collects the code from there a few seconds later
	 * (`tools/connection.rs`). So coming back to the app reloads the list
	 * until the new connection shows, for up to half a minute.
	 */
	function reloadUntilConnected() {
		const before = connected.length;
		let tries = 0;
		const again = async () => {
			await loadConnections();
			if (connected.length === before && ++tries < 15) setTimeout(again, 2000);
		};
		reloadOnReturn(again);
	}

	/** Starts a service's sign-in and opens it in the browser. */
	async function connectService(target: { registry: string } | { url: string }, key: string) {
		busy = key;
		connectionsError = null;
		try {
			// `sheet` asks the proxy to close the iOS sign-in sheet once the
			// sign-in lands there.
			const sheet = await canSignInSheet();
			const { auth_url } = await apiSend<{ auth_url: string }>('POST', '/connections/connect', {
				...target,
				sheet
			});
			reloadUntilConnected();
			await openSignIn(auth_url);
		} catch (e) {
			connectionsError = message(e);
		} finally {
			busy = null;
		}
	}

	async function disconnectService(id: string) {
		busy = id;
		connectionsError = null;
		try {
			await apiSend('DELETE', `/connections/${encodeURIComponent(id)}`);
			await loadConnections();
		} catch (e) {
			connectionsError = message(e);
		} finally {
			busy = null;
		}
	}
</script>

<Page
	title="Catalog"
	description="Services you can connect. Some sync your data to your server. In others, the assistant can act for you when you ask in chat."
	maxWidth="wide"
>
	{#if store.error}
		<div class="error">{store.error}</div>
	{/if}
	{#if connectFlow.error}
		<div class="error">{connectFlow.error}</div>
	{/if}
	{#if connectionsError}
		<div class="error">{connectionsError}</div>
	{/if}

	<UniversalDataGrid
		items={rows}
		{columns}
		entityType="source"
		loading={store.loading}
		error={null}
		emptyIcon="mynaui:power"
		emptyMessage="No sources in the catalog"
		loadingMessage="Loading sources…"
		searchPlaceholder="Search services…"
		bind:searchQuery={query}
		defaultViewMode="table"
		onItemClick={openSource}
		onRetry={() => store.load()}
	>
		{#snippet rowActions(row: Row)}
			{#if row.source}
				<Button variant="secondary" size="sm" onclick={() => void connect(row.source!)}>
					{connectLabel(row)}
				</Button>
			{:else if row.service?.id}
				{@const id = row.service.id}
				<TextAction loading={busy === id} disabled={busy !== null} onclick={() => void disconnectService(id)}>
					Disconnect
				</TextAction>
			{:else if row.service?.registry}
				{@const registry = row.service.registry}
				<Button
					variant="secondary"
					size="sm"
					loading={busy === registry}
					disabled={busy !== null}
					onclick={() => void connectService({ registry }, registry)}
				>
					Connect
				</Button>
			{/if}
		{/snippet}

	</UniversalDataGrid>

	{#if registryError}
		<p class="quiet">{registryError}</p>
	{/if}

	{#if extraResults}
		<section class="more">
			<h2 class="heading">From the MCP Registry</h2>
			<p class="quiet">
				Anyone can publish a server to the registry. Connect only services you already trust.
				Servers that need an API key are not listed yet.
			</p>
			{#if extraResults.length === 0}
				<p class="quiet">
					{searching ? 'Searching the registry…' : 'No other server you can connect matches that.'}
				</p>
			{:else}
				<ul class="listings">
					{#each extraResults as l (l.registry)}
						<li>
							<div class="text">
								<span class="name">{l.title}</span>
								<span class="about">{ACTS}.{l.about ? ` ${l.about}` : ''}</span>
							</div>
							<Button
								variant="secondary"
								size="sm"
								loading={busy === l.registry}
								disabled={busy !== null}
								onclick={() => void connectService({ registry: l.registry }, l.registry)}
							>
								Connect
							</Button>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}

	<section class="more">
		<h2 class="heading">Add a service by its address</h2>
		<p class="quiet">
			For a service's MCP server that is not listed. The assistant can act there once you sign in.
			Its address starts with https://.
		</p>
		<div class="add">
			<Input type="url" bind:value={address} placeholder="https://mcp.example.com/mcp" />
			<Button
				variant="secondary"
				loading={busy === 'address'}
				disabled={busy !== null || !address.trim()}
				onclick={() => void connectService({ url: address.trim() }, 'address')}
			>
				Connect
			</Button>
		</div>
	</section>

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
		border-radius: var(--radius-md);
		background: var(--color-error-subtle);
		color: color-mix(in srgb, var(--color-error) 75%, #000);
		font-size: 0.8125rem;
	}

	.more {
		margin-top: 2rem;
		max-width: 44rem;
	}
	.heading {
		margin: 0 0 0.375rem;
		font-size: 0.9375rem;
		font-weight: 600;
		color: var(--color-foreground);
	}
	.quiet {
		margin: 0 0 0.75rem;
		font-size: 0.8125rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.listings {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.listings li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.75rem 0;
		border-bottom: 1px solid var(--color-border-subtle);
	}
	.listings li:last-child {
		border-bottom: 0;
	}
	.text {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}
	.name {
		font-weight: 500;
		color: var(--color-foreground);
	}
	.about {
		font-size: 0.8125rem;
		line-height: 1.4;
		color: var(--color-foreground-muted);
		overflow: hidden;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
	}
	.add {
		display: flex;
		gap: 0.5rem;
		align-items: flex-start;
	}
	.add > :global(:first-child) {
		flex: 1;
	}

	.uncovered {
		margin: 1rem 0 0;
		max-width: 44rem;
		font-size: 0.8125rem;
		line-height: 1.55;
		color: var(--color-foreground-subtle, #9ca3af);
	}

</style>
