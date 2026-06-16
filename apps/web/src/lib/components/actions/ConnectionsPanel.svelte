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
	import { spaceStore } from '$lib/stores/space.svelte';
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
		oauthStart,
		type Credential,
		type SourceCatalogItem
	} from '$lib/api/client';
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
				const { redirect_url } = await oauthStart(source.id, {
					return_url: `${window.location.origin}/oauth/callback`
				});
				window.location.assign(redirect_url);
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
		spaceStore.openTabFromRoute(`/sources/${row.id}`);
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
			label: 'Actions',
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

<style>
	.sources-page {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
		padding: 1.25rem 1.5rem 2rem;
		max-width: 1100px;
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
		background: #fee2e2;
		color: #991b1b;
		font-size: 0.8125rem;
	}
</style>
