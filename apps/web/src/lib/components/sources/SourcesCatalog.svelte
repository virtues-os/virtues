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
	is for. Connections hang off the row as its expansion, and both kinds are
	treated alike (see lib/stores/sources.svelte.ts).
-->
<script lang="ts">
	import { Page } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import UniversalDataGrid, {
		type Column
	} from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { sourcesStore, type Connection } from '$lib/stores/sources.svelte';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { relativeTime } from '$lib/applets/palette';
	import { isMacOS, isTauri, thisComputerLabel } from '$lib/utils/platform';
	import { openExternal } from '$lib/tauri/bridge';
	import type { SourceCatalogItem } from '$lib/api/client';

	const store = sourcesStore;

	$effect(() => {
		void store.load();
	});

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
		source: SourceCatalogItem;
	};

	const AUTH_LABEL: Record<string, string> = {
		self_issued_bearer: 'Device',
		via_proxy: 'Account',
		api_key: 'Key'
	};

	const rows = $derived.by<Row[]>(() =>
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
				source
			};
		})
	);

	const columns: Column<Row>[] = [
		{ key: 'name', label: 'Source', icon: 'ri:plug-line', width: '26%', minWidth: '150px' },
		{
			key: 'kind',
			label: 'Connects by',
			icon: 'ri:key-2-line',
			width: '14%',
			minWidth: '110px',
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
	 * not pairing, and ThisMacView owns that. Handing the user a six-digit code
	 * to carry to the app they are currently *using* is the narrative break this
	 * avoids.
	 */
	function isThisDevice(source: SourceCatalogItem): boolean {
		return isTauri && isMacOS && source.id === 'mac';
	}

	function connectLabel(row: Row): string {
		if (isThisDevice(row.source)) return `Set up ${thisComputerLabel}`;
		if (row.source.auth_kind === 'self_issued_bearer') return 'Pair a device';
		return row.connected > 0 ? 'Connect another' : 'Connect';
	}

	async function connect(source: SourceCatalogItem) {
		if (isThisDevice(source)) {
			windowShellStore.navigate('/virtues/this-mac', { label: 'This Mac' });
			return;
		}
		await connectFlow.start(source);
	}

	function openConnection(c: Connection) {
		if (c.route) windowShellStore.navigate(c.route, { label: c.name });
	}
</script>

<Page
	title="Catalog"
	description="Everything Virtues can draw from, and what you have connected to each. Open a row to see its connections."
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
		onRetry={() => store.load()}
	>
		{#snippet rowActions(row: Row)}
			<button type="button" class="connect" onclick={() => void connect(row.source)}>
				{connectLabel(row)}
			</button>
		{/snippet}

		{#snippet expandDetail(row: Row)}
			<div class="detail">
				<p class="desc">{row.description}</p>

				{#if row.source.repo}
					<!-- Provenance, not an install path. The collectors arrive through
					     the App Store and update themselves; this is so you can read
					     what they do before pairing one to your life. -->
					<p class="repo">
						<Icon icon="ri:code-line" width="14" />
						<button
							type="button"
							class="link"
							onclick={() =>
								void openExternal(
									row.source.repo_ref
										? `${row.source.repo}/tree/main/${row.source.repo_ref}`
										: (row.source.repo as string)
								)}
						>
							Read the code
						</button>
						{#if row.source.repo_ref}<code>{row.source.repo_ref}</code>{/if}
					</p>
				{/if}
				{#if row.connections.length === 0}
					<p class="none">
						Nothing connected yet.
						<button type="button" class="link" onclick={() => void connect(row.source)}>
							{connectLabel(row)}
						</button>
					</p>
				{:else}
					<ul class="connections">
						{#each row.connections as c (c.id)}
							<li class="connection" class:broken={c.broken}>
								<span class="dot" aria-hidden="true"></span>
								<button
									type="button"
									class="cname"
									class:inert={!c.route}
									disabled={!c.route}
									onclick={() => openConnection(c)}
								>
									{c.name}{c.isCurrent ? ' · this device' : ''}
								</button>
								<span class="cstatus">{c.statusLabel}</span>
								<span class="cseen">
									{c.lastSeenAt ? relativeTime(c.lastSeenAt) : 'no activity yet'}
								</span>
							</li>
							{#if c.statusReason}
								<li class="reason">
									<Icon icon="ri:error-warning-line" width="14" />
									<span>{c.statusReason}</span>
								</li>
							{/if}
						{/each}
					</ul>
				{/if}
			</div>
		{/snippet}
	</UniversalDataGrid>
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

	.detail {
		padding: 0.625rem 0.5rem 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.desc {
		margin: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.none {
		margin: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.repo {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		margin: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.repo code {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.link {
		border: none;
		background: none;
		padding: 0;
		margin-left: 0.25rem;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
	.link:hover {
		text-decoration: underline;
	}

	.connections {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.connection {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.3125rem 0;
		font-size: 0.75rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-success, #16a34a);
		flex-shrink: 0;
	}
	.connection.broken .dot {
		background: var(--color-error);
	}
	.cname {
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
	.cname:hover:not(.inert) {
		text-decoration: underline;
	}
	.cname.inert {
		cursor: default;
	}
	.cstatus {
		flex-shrink: 0;
		width: 9rem;
	}
	.connection.broken .cstatus {
		color: var(--color-error);
		font-weight: 500;
	}
	.cseen {
		flex-shrink: 0;
		width: 9rem;
	}

	.reason {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0 0 0.375rem 1rem;
		font-size: 0.6875rem;
		color: color-mix(in srgb, var(--color-error) 75%, #000);
	}

	@media (max-width: 640px) {
		.cstatus,
		.cseen {
			display: none;
		}
	}
</style>
