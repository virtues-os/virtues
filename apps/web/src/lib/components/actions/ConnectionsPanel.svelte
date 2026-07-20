<!--
	ConnectionsPanel.svelte — the /sources page (also mounted at /actions#sources).

	Single UniversalDataGrid of connected credentials. The catalog of available
	sources is reachable via a "+ Connect" button (SourceConnectButton) that
	drops a popover anchored to the trigger — no modal, no backdrop. This keeps
	the page focused on managing what's already wired up; the catalog is a
	transient picker, not a permanent shelf.

	Vocabulary: each row is a *credential* (one connection to a provider).
	Each credential fans out one or more *actions* that run on a schedule
	(or webhook for self_issued_bearer devices) and write to data_* tables.
-->

<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import DevicePairModal from '$lib/components/sources/DevicePairModal.svelte';
	import ApiKeyConnectModal from '$lib/components/sources/ApiKeyConnectModal.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import ChatImportCard from '$lib/components/onboarding/ChatImportCard.svelte';
	import SourceConnectButton from '$lib/components/sources/SourceConnectButton.svelte';
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
	import { startOAuth, reloadOnReturn } from '$lib/components/sources/connectDispatch';
	import { relativeTime } from '$lib/actions/palette';

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

	async function handleConnect(source: SourceCatalogItem) {
		err = null;

		if (source.auth_kind === 'self_issued_bearer') {
			pairModalDeviceType = source.id === 'mac' ? 'mac' : 'ios';
			pairModalDisplayName = source.name;
			pairModalOpen = true;
			return;
		}

		if (source.auth_kind === 'via_proxy') {
			try {
				const { external } = await startOAuth(source.id);
				// Tauri: the SPA stayed mounted (system browser handled the dance);
				// refresh the credential list when the user switches back.
				if (external) reloadOnReturn(load);
			} catch (e) {
				err = e instanceof Error ? e.message : String(e);
			}
			return;
		}

		// One-time import sources open the upload card, not the api-key form.
		if (source.id === 'chat_import') {
			chatImportOpen = true;
			return;
		}

		if (source.auth_kind === 'api_key') {
			apikeyModalSource = source;
			apikeyModalOpen = true;
			return;
		}

		err = `Unknown auth_kind for "${source.name}": ${source.auth_kind}`;
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
	const rows = $derived.by<CredRow[]>(() =>
		credentials
			.filter((c) => c.status !== 'pending')
			.map((c) => ({
				...c,
				source_label: sourceLabel(c.provider),
				// Active rows show the Tier-2 init-sync lifecycle
				// (connected → backfilling → live); others show the raw status.
				status_label: c.status === 'active' ? (c.sync_state ?? 'active') : c.status,
				last_seen_label: c.last_seen_at
					? relativeTime(c.last_seen_at)
					: c.status === 'active'
						? 'no activity yet'
						: `revoked ${relativeTime(c.created_at)}`
			}))
	);

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
				revoked: 'badge-muted'
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
			key: 'action_count',
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
				<em>actions</em> that run on a schedule (or on-device webhook) and write into
				your data tables. Connect a source to start ingestion.
			</p>
		</div>
		<div class="actions">
			<SourceConnectButton {catalog} onPick={handleConnect} align="right" />
		</div>
	</header>

	{#if err}
		<div class="error">{err}</div>
	{/if}

	{#if !loading && credentials.length === 0}
		<div class="empty-hero">
			<Icon icon="ri:plug-line" width="32" />
			<h2>No sources connected yet</h2>
			<p>
				Pick a provider to start ingesting data. Each source creates the
				actions that pull or receive its data.
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
				<span class="manage-status" class:revoked={manageCred.status !== 'active'}>
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
					<div><dt>Actions</dt><dd>{manageCred.action_count}</dd></div>
				</dl>
			{:else}
				<dl class="manage-info">
					<div><dt>Last seen</dt><dd>{manageCred.last_seen_label}</dd></div>
					<div><dt>Actions</dt><dd>{manageCred.action_count}</dd></div>
				</dl>
			{/if}

			{#if manageCred.status === 'active'}
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

			{#if manageCred.status === 'active'}
				<div class="manage-danger">
					{#if confirmingDisconnect}
						<p class="danger-prompt">
							Disconnect this {manageCred.source_label}? It stops ingesting and its
							actions are removed. Re-pair to reconnect.
						</p>
						<div class="danger-actions">
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
	.actions {
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
	.danger-actions {
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
