<!--
	ConnectionsPanel.svelte — the /sources page (also mounted at /applets#sources).

	Single UniversalDataGrid of connected credentials. The catalog of available
	sources is reachable via a "+ Connect" button (SourceConnectButton) that
	drops a popover anchored to the trigger — no modal, no backdrop. This keeps
	the page focused on managing what's already wired up; the catalog is a
	transient picker, not a permanent shelf.

	Vocabulary: each row is a *credential* (one connection to a provider).
	Each credential fans out one or more *applets* that run on a schedule
	(or webhook for self_issued_bearer devices) and write to data_* tables.
-->

<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import DevicePairModal from '$lib/components/sources/DevicePairModal.svelte';
	import ApiKeyConnectModal from '$lib/components/sources/ApiKeyConnectModal.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import ChatImportCard from '$lib/components/onboarding/ChatImportCard.svelte';
	import SourceConnectButton from '$lib/components/sources/SourceConnectButton.svelte';
	import StreamHealthPanel from '$lib/components/sources/StreamHealthPanel.svelte';
	import UniversalDataGrid, {
		type Column
	} from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import {
		listCredentials,
		listSourceCatalog,
		renameCredential,
		revokeCredential,
		type Credential,
		type SourceCatalogItem
	} from '$lib/api/client';
	import { connectIntent, reloadOnReturn } from '$lib/components/sources/connectDispatch';
	import { relativeTime } from '$lib/applets/palette';

	// ────────────────────────────────────────────────────────────────────────
	// State
	// ────────────────────────────────────────────────────────────────────────

	let credentials = $state<Credential[]>([]);
	let catalog = $state<SourceCatalogItem[]>([]);
	let loading = $state(true);
	let err = $state<string | null>(null);

	let pairModalOpen = $state(false);
	let pairModalDeviceType = $state<'ios' | 'mac'>('ios');
	let pairModalDisplayName = $state('iPhone');

	let apikeyModalOpen = $state(false);
	let apikeyModalSource = $state<SourceCatalogItem | null>(null);

	let chatImportOpen = $state(false);

	// Manage-connection modal (opened by clicking a credential row). Holds the
	// rename + disconnect affordances — the only place to CRUD a connection.
	let manageOpen = $state(false);
	let manageCred = $state<CredRow | null>(null);
	let renameValue = $state('');
	let manageBusy = $state(false);
	let manageErr = $state<string | null>(null);
	let confirmingDisconnect = $state(false);

	// ────────────────────────────────────────────────────────────────────────
	// Data loading
	// ────────────────────────────────────────────────────────────────────────

	// The OAuth callback 302s back here as `?connected=<source_id>` on success
	// and `?source=<id>&error=<reason>` when it didn't finish — and nothing read
	// either, so a round-trip through the provider ended in silence whichever
	// way it went. Read once at mount and strip the params, so a refresh doesn't
	// replay a stale verdict. (Native shells never land here; they get a
	// terminal page in the system browser instead.)
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
			return { ok: true, text: `${sourceLabel(connectReturn.connected)} is connected.` };
		}
		const who = connectReturn.source ? sourceLabel(connectReturn.source) : 'That source';
		return {
			ok: false,
			text:
				connectReturn.error === 'connect_cancelled'
					? `${who} wasn't connected — the flow was closed before it finished.`
					: `Couldn't finish connecting ${who}. Nothing was connected.`
		};
	});

	async function load() {
		loading = true;
		err = null;
		try {
			const [cs, src] = await Promise.all([listCredentials(), listSourceCatalog()]);
			credentials = cs;
			catalog = src;
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
	});

	// ────────────────────────────────────────────────────────────────────────
	// Catalog lookups (provider → display name + icon)
	// ────────────────────────────────────────────────────────────────────────

	const catalogById = $derived.by(() => {
		const m = new Map<string, SourceCatalogItem>();
		for (const s of catalog) m.set(s.id, s);
		return m;
	});

	function sourceLabel(provider: string): string {
		return catalogById.get(provider)?.name ?? provider;
	}

	// ────────────────────────────────────────────────────────────────────────
	// Connect dispatch — picker → right modal/redirect for each auth_kind
	// ────────────────────────────────────────────────────────────────────────

	// Dispatch lives in connectDispatch so this page and onboarding cannot
	// disagree about what a source click does — the two had already drifted on
	// where chat_import is tested, which is the drift that module exists to
	// prevent. This turns the intent into modal state; nothing decides here.
	async function handleConnect(source: SourceCatalogItem) {
		err = null;
		const intent = await connectIntent(source);
		switch (intent.kind) {
			case 'pair':
				pairModalDeviceType = intent.deviceType;
				pairModalDisplayName = intent.displayName;
				pairModalOpen = true;
				return;
			case 'chat_import':
				chatImportOpen = true;
				return;
			case 'api_key':
				apikeyModalSource = intent.source;
				apikeyModalOpen = true;
				return;
			case 'oauth':
				// Tauri: the SPA stayed mounted (system browser handled the dance);
				// refresh the credential list when the user switches back.
				if (intent.external) reloadOnReturn(load);
				return;
			case 'error':
				err = intent.message;
				return;
		}
	}

	/**
	 * Re-run the original connect flow for a credential that has gone bad.
	 * Reconnecting *is* connecting — a fresh OAuth dance or a fresh key against
	 * the same source — so this reuses the dispatcher rather than inventing a
	 * repair path. The old row is replaced by the provider's own upsert.
	 */
	async function doReconnect(cred: CredRow | null = manageCred) {
		if (!cred) return;
		const source = catalogById.get(cred.provider);
		if (!source) {
			manageErr = `No catalog entry for "${cred.provider}"`;
			return;
		}
		closeManage();
		await handleConnect(source);
	}

	function handleRowClick(row: CredRow) {
		manageCred = row;
		renameValue = row.name;
		manageErr = null;
		confirmingDisconnect = false;
		manageOpen = true;
	}

	function closeManage() {
		manageOpen = false;
		manageCred = null;
		manageBusy = false;
		confirmingDisconnect = false;
		manageErr = null;
	}

	async function doRename() {
		if (!manageCred) return;
		const next = renameValue.trim();
		if (!next || next === manageCred.name) return;
		manageBusy = true;
		manageErr = null;
		try {
			await renameCredential(manageCred.id, next);
			closeManage();
			await load();
		} catch (e) {
			manageErr = e instanceof Error ? e.message : String(e);
			manageBusy = false;
		}
	}

	async function doDisconnect() {
		if (!manageCred) return;
		manageBusy = true;
		manageErr = null;
		try {
			await revokeCredential(manageCred.id);
			closeManage();
			await load();
		} catch (e) {
			manageErr = e instanceof Error ? e.message : String(e);
			manageBusy = false;
			confirmingDisconnect = false;
		}
	}

	// ────────────────────────────────────────────────────────────────────────
	// Grid columns
	// ────────────────────────────────────────────────────────────────────────

	type CredRow = Credential & {
		// Synthetic fields for sort/search/display via UniversalDataGrid.
		source_label: string;
		status_label: string;
		last_seen_label: string;
	};

	// Filter out `pending` rows (transient pre-pairing state — the server
	// minted them at pair_initiate and they get hard-deleted on cancel or
	// flipped to `active` on complete). They should never surface in the UI.
	/** A connection that has stopped working and needs the user, not us. */
	function isBroken(status: string): boolean {
		return status === 'reauth_required' || status === 'error';
	}

	// Active rows show the Tier-2 init-sync lifecycle (connected → backfilling
	// → live). The rest show their status in the user's terms: `reauth_required`
	// is the provider asking them to sign in again, which is an instruction, not
	// a state name.
	function statusLabel(c: Credential): string {
		if (c.status === 'active') return c.sync_state ?? 'active';
		if (c.status === 'reauth_required') return 'sign in again';
		return c.status;
	}

	function lastSeenLabel(c: Credential): string {
		if (c.last_seen_at) return relativeTime(c.last_seen_at);
		if (c.status === 'active') return 'no activity yet';
		if (isBroken(c.status)) return 'not delivering';
		return `revoked ${relativeTime(c.created_at)}`;
	}

	const rows = $derived.by<CredRow[]>(() =>
		credentials
			.filter((c) => c.status !== 'pending')
			.map((c) => ({
				...c,
				source_label: sourceLabel(c.provider),
				status_label: statusLabel(c),
				last_seen_label: lastSeenLabel(c)
			}))
	);

	// Broken connections are the only thing on this page the user must act on,
	// and a row in a sorted grid is easy to miss. Surfaced above the grid with
	// the provider's own reason where we have it.
	const broken = $derived(rows.filter((c) => isBroken(c.status)));

	const columns: Column<CredRow>[] = [
		{
			key: 'source_label',
			label: 'Source',
			icon: 'ri:plug-line',
			width: '20%',
			minWidth: '140px'
		},
		{
			key: 'name',
			label: 'Name',
			icon: 'ri:bookmark-line',
			width: '30%',
			minWidth: '180px'
		},
		{
			key: 'status_label',
			label: 'Status',
			icon: 'ri:circle-line',
			width: '15%',
			minWidth: '100px',
			format: 'badge',
			badgeColors: {
				live: 'badge-success',
				active: 'badge-success',
				backfilling: 'badge-warning',
				connected: 'badge-muted',
				revoked: 'badge-muted',
				'sign in again': 'badge-error',
				error: 'badge-error'
			}
		},
		{
			key: 'last_seen_label',
			label: 'Last seen',
			icon: 'ri:time-line',
			width: '20%',
			minWidth: '120px',
			hideOnMobile: true
		},
		{
			key: 'applet_count',
			label: 'Applets',
			icon: 'ri:flashlight-line',
			width: '15%',
			minWidth: '90px',
			format: 'number'
		}
	];
</script>

<section class="sources-page">
	<header class="page-header">
		<div class="title-block">
			<h1>Sources</h1>
			<p class="subtitle">
				Sources are where data comes from. Each connected source creates one or more
				<em>applets</em> that run on a schedule (or on-device webhook) and write into
				your data tables. Connect a source to start ingestion.
			</p>
		</div>
		<div class="applets">
			<SourceConnectButton {catalog} onPick={handleConnect} align="right" />
		</div>
	</header>

	{#if err}
		<div class="error">{err}</div>
	{/if}

	{#if notice}
		<div class="notice" class:ok={notice.ok}>
			<Icon icon={notice.ok ? 'ri:check-line' : 'ri:information-line'} width="16" />
			<span>{notice.text}</span>
			<button type="button" class="notice-x" onclick={() => (noticeDismissed = true)} aria-label="Dismiss">
				<Icon icon="ri:close-line" width="15" />
			</button>
		</div>
	{/if}

	<!-- Above even the flow panel: a stopped stream is a fact to read, but a
	     credential the provider has locked out is a job only the user can do. -->
	{#if broken.length > 0}
		<ul class="attention">
			{#each broken as c (c.id)}
				<li>
					<Icon icon="ri:error-warning-line" width="16" />
					<div class="what">
						<span class="who">{c.source_label} · {c.name}</span>
						<span class="why">
							{c.status_reason ??
								(c.status === 'reauth_required'
									? 'The provider needs you to sign in again.'
									: 'This connection stopped working.')}
						</span>
					</div>
					<button type="button" class="reconnect" onclick={() => void doReconnect(c)}>
						Reconnect
					</button>
				</li>
			{/each}
		</ul>
	{/if}

	<!-- Connecting a source is only half the story; this is whether it's still
	     delivering. Sits above the source list because a stopped stream is more
	     urgent than the roster of what's plugged in. -->
	<div class="flow-health">
		<StreamHealthPanel />
	</div>

	{#if !loading && credentials.length === 0}
		<div class="empty-hero">
			<Icon icon="ri:plug-line" width="32" />
			<h2>No sources connected yet</h2>
			<p>
				Pick a provider to start ingesting data. Each source creates the
				applets that pull or receive its data.
			</p>
			<SourceConnectButton
				{catalog}
				onPick={handleConnect}
				align="center"
				label="Connect a source"
			/>
		</div>
	{:else}
		<UniversalDataGrid
			items={rows}
			{columns}
			entityType="credential"
			{loading}
			error={null}
			emptyIcon="ri:plug-line"
			emptyMessage="No sources connected yet"
			loadingMessage="Loading sources..."
			searchPlaceholder="Search by source or name..."
			defaultViewMode="table"
			animateMount={true}
			onItemClick={handleRowClick}
			onRetry={load}
		/>
	{/if}
</section>

<DevicePairModal
	deviceType={pairModalDeviceType}
	displayName={pairModalDisplayName}
	open={pairModalOpen}
	onClose={() => {
		pairModalOpen = false;
		void load();
	}}
	onSuccess={() => {
		pairModalOpen = false;
		void load();
	}}
/>

<ApiKeyConnectModal
	source={apikeyModalSource}
	open={apikeyModalOpen}
	onClose={() => (apikeyModalOpen = false)}
	onSuccess={() => {
		apikeyModalOpen = false;
		void load();
	}}
/>

<Modal open={chatImportOpen} onClose={() => { chatImportOpen = false; void load(); }} title="Import chat history" width="md">
	<ChatImportCard />
</Modal>

<Modal open={manageOpen} onClose={closeManage} title="Manage connection" width="sm">
	{#if manageCred}
		<div class="manage">
			<div class="manage-head">
				<Icon icon={catalogById.get(manageCred.provider)?.icon ?? 'ri:plug-line'} width="20" />
				<span class="manage-source">{manageCred.source_label}</span>
				<span
					class="manage-status"
					class:revoked={manageCred.status === 'revoked'}
					class:broken={isBroken(manageCred.status)}
				>
					{manageCred.status_label}
				</span>
			</div>

			{#if manageCred.device_info}
				<dl class="manage-info">
					<div><dt>Device</dt><dd>{manageCred.device_info.device_model || manageCred.device_info.device_name}</dd></div>
					<div><dt>OS</dt><dd>{manageCred.device_info.os_version}</dd></div>
					{#if manageCred.device_info.app_version}
						<div><dt>App</dt><dd>{manageCred.device_info.app_version}</dd></div>
					{/if}
					<div><dt>Last seen</dt><dd>{manageCred.last_seen_label}</dd></div>
					<div><dt>Applets</dt><dd>{manageCred.applet_count}</dd></div>
				</dl>
			{:else}
				<dl class="manage-info">
					<div><dt>Last seen</dt><dd>{manageCred.last_seen_label}</dd></div>
					<div><dt>Applets</dt><dd>{manageCred.applet_count}</dd></div>
				</dl>
			{/if}

			{#if isBroken(manageCred.status)}
				<div class="manage-broken">
					<p>
						{manageCred.status_reason ??
							(manageCred.status === 'reauth_required'
								? 'The provider needs you to sign in again before this can deliver.'
								: 'This connection stopped working.')}
					</p>
					<button class="btn-secondary" onclick={() => void doReconnect()} disabled={manageBusy}>
						<Icon icon="ri:refresh-line" width="15" />
						Reconnect
					</button>
				</div>
			{/if}

			{#if manageCred.status !== 'revoked'}
				<label class="manage-rename">
					<span>Name</span>
					<div class="rename-row">
						<input
							type="text"
							bind:value={renameValue}
							placeholder="e.g. My iPhone"
							disabled={manageBusy}
						/>
						<button
							class="btn-secondary"
							onclick={doRename}
							disabled={manageBusy || !renameValue.trim() || renameValue.trim() === manageCred.name}
						>
							Save
						</button>
					</div>
				</label>
			{/if}

			{#if manageErr}
				<div class="error">{manageErr}</div>
			{/if}

			{#if manageCred.status !== 'revoked'}
				<div class="manage-danger">
					{#if confirmingDisconnect}
						<p class="danger-prompt">
							Disconnect this {manageCred.source_label}? It stops ingesting and its
							applets are removed. Re-pair to reconnect.
						</p>
						<div class="danger-applets">
							<button class="btn-ghost" onclick={() => (confirmingDisconnect = false)} disabled={manageBusy}>
								Cancel
							</button>
							<button class="btn-danger" onclick={doDisconnect} disabled={manageBusy}>
								{manageBusy ? 'Disconnecting…' : 'Disconnect'}
							</button>
						</div>
					{:else}
						<button class="btn-danger-outline" onclick={() => (confirmingDisconnect = true)} disabled={manageBusy}>
							<Icon icon="ri:link-unlink-m" width="15" />
							Disconnect
						</button>
					{/if}
				</div>
			{/if}
		</div>
	{/if}
</Modal>

<style>
	.sources-page {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
		padding: 1.25rem 1.5rem 2rem;
		max-width: 72rem;
		width: 100%;
		margin: 0 auto;
	}

	.page-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1.5rem;
	}
	.title-block {
		flex: 1;
		min-width: 0;
	}
	.page-header h1 {
		margin: 0;
		font-size: 1.5rem;
		font-weight: 600;
	}
	.subtitle {
		margin: 0.375rem 0 0;
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle, #9ca3af);
		max-width: 60ch;
	}
	.subtitle em {
		font-style: italic;
		color: var(--color-foreground-muted, #6b7280);
	}
	.applets {
		flex-shrink: 0;
	}

	.empty-hero {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		padding: 4rem 1rem;
		text-align: center;
		color: var(--color-foreground-muted, #6b7280);
	}
	.empty-hero h2 {
		margin: 0.25rem 0 0;
		font-size: 1.0625rem;
		font-weight: 600;
		color: var(--color-foreground, #111827);
	}
	.empty-hero p {
		margin: 0;
		max-width: 44ch;
		font-size: 0.875rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.error {
		padding: 0.5rem 0.75rem;
		border-radius: 6px;
		background: var(--color-error-subtle);
		color: color-mix(in srgb, var(--color-error) 75%, #000);
		font-size: 0.8125rem;
	}

	.flow-health {
		margin-bottom: 1.25rem;
	}

	.notice {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
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

	/* ── Broken connections ───────────────────────────────────────────────── */
	.attention {
		list-style: none;
		margin: 0;
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
	.attention .what {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.0625rem;
	}
	.attention .who {
		font-size: 0.8125rem;
		font-weight: 600;
	}
	.attention .why {
		font-size: 0.75rem;
		opacity: 0.85;
	}
	.reconnect {
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
	.reconnect:hover {
		background: color-mix(in srgb, var(--color-error) 12%, transparent);
	}

	.manage-broken {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.5rem;
		padding: 0.625rem 0.75rem;
		border-radius: 6px;
		background: var(--color-error-subtle);
	}
	.manage-broken p {
		margin: 0;
		font-size: 0.8125rem;
		color: color-mix(in srgb, var(--color-error) 75%, #000);
	}

	/* ── Manage-connection modal ──────────────────────────────────────────── */
	.manage {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	.manage-head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.9375rem;
		font-weight: 600;
		color: var(--color-foreground, #111827);
	}
	.manage-source {
		flex: 1;
		min-width: 0;
	}
	.manage-status {
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		padding: 0.125rem 0.5rem;
		border-radius: var(--radius-full);
		background: var(--color-success-subtle, #dcfce7);
		color: var(--color-success, #166534);
	}
	.manage-status.revoked {
		background: var(--color-muted, #f3f4f6);
		color: var(--color-foreground-muted, #6b7280);
	}
	.manage-status.broken {
		background: var(--color-error-subtle);
		color: color-mix(in srgb, var(--color-error) 75%, #000);
	}
	.manage-info {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.5rem 1rem;
		margin: 0;
	}
	.manage-info div {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}
	.manage-info dt {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.manage-info dd {
		margin: 0;
		font-size: 0.8125rem;
		color: var(--color-foreground, #111827);
	}
	.manage-rename {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}
	.manage-rename > span {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.rename-row {
		display: flex;
		gap: 0.5rem;
	}
	.rename-row input {
		flex: 1;
		min-width: 0;
		padding: 0.4375rem 0.625rem;
		border: 1px solid var(--color-border, #d1d5db);
		border-radius: 6px;
		font-size: 0.8125rem;
		background: var(--color-background, #fff);
		color: var(--color-foreground, #111827);
	}
	.manage-danger {
		border-top: 1px solid var(--color-border, #e5e7eb);
		padding-top: 0.875rem;
	}
	.danger-prompt {
		margin: 0 0 0.625rem;
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-muted, #6b7280);
	}
	.danger-applets {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
	}
	.btn-secondary,
	.btn-ghost,
	.btn-danger,
	.btn-danger-outline {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.4375rem 0.75rem;
		border-radius: 6px;
		font-size: 0.8125rem;
		font-weight: 500;
		cursor: pointer;
		border: 1px solid transparent;
	}
	.btn-secondary {
		background: var(--color-muted, #f3f4f6);
		color: var(--color-foreground, #111827);
		border-color: var(--color-border, #d1d5db);
	}
	.btn-ghost {
		background: transparent;
		color: var(--color-foreground-muted, #6b7280);
	}
	.btn-danger {
		background: var(--color-error);
		color: #fff;
	}
	.btn-danger-outline {
		background: transparent;
		color: color-mix(in srgb, var(--color-error) 75%, #000);
		border-color: var(--color-error);
	}
	.btn-secondary:disabled,
	.btn-danger:disabled,
	.btn-danger-outline:disabled,
	.btn-ghost:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
