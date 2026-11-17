<!--
	SourcesView — the Sources room, opened from its button in the window's
	corner (HeaderCorner), beside Search and Settings.

	Five sections, across the top, in the order the questions get asked:

	  Overview  — is anything broken right now, and is data still arriving
	  Catalog   — what can I plug in, and what is already plugged in
	  Activity  — what has actually been running
	  Streams   — every stream your server keeps, and how full each is
	  Query     — SQL against your own record

	Section is a pure function of the route, and the reserved words live in
	the registry beside the matcher (SOURCES_SECTIONS), so the router and this
	file cannot disagree about whether `/sources/catalog` is a section or a
	credential id. One source's page and one connection's page carry no strip:
	they are reached from a section, and Back returns to it.
-->
<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { SOURCES_SECTIONS } from '$lib/tabs/registry';
	import Modal from '$lib/components/Modal.svelte';
	import ChatImportCard from '$lib/components/onboarding/ChatImportCard.svelte';
	import DevicePairModal from './DevicePairModal.svelte';
	import ApiKeyConnectModal from './ApiKeyConnectModal.svelte';
	import SourcesOverview from './SourcesOverview.svelte';
	import SourcesCatalog from './SourcesCatalog.svelte';
	import SourcesActivity from './SourcesActivity.svelte';
	import SourceDetail from './SourceDetail.svelte';
	import SubNav from '$lib/components/SubNav.svelte';
	import StreamsView from '$lib/components/tabs/views/StreamsView.svelte';
	import DeveloperSqlView from '$lib/components/tabs/views/DeveloperSqlView.svelte';
	import CredentialDetailView from '$lib/components/tabs/views/CredentialDetailView.svelte';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { reloadOnReturn } from './connectDispatch';

	let { tab, active = true }: { tab: Tab; active?: boolean } = $props();

	const SECTION_ITEMS = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'catalog', label: 'Catalog' },
		{ id: 'activity', label: 'Activity' },
		{ id: 'streams', label: 'Streams' },
		{ id: 'query', label: 'Query' },
	];

	// Three shapes share this slot, discriminated without a lookup:
	//   ''                  → Overview
	//   a reserved word     → that section
	//   `cred_…`            → one connection
	//   anything else       → one source (catalog ids are bare slugs)
	// Keying the connection case on the id prefix rather than on "is this in the
	// catalog" keeps the dispatch synchronous — a source that is no longer
	// installed still needs a page, precisely so its leftover connections can be
	// found and removed.
	const seg = $derived(tab.route.replace(/^\/sources\/?/, '').split('/')[0]);

	const section = $derived.by(() => {
		if (seg === '') return 'overview';
		if ((SOURCES_SECTIONS as readonly string[]).includes(seg)) return seg;
		return seg.startsWith('cred_') ? 'connection' : 'source';
	});

	// The modals live here rather than in a section so that finishing a connect
	// doesn't depend on which section you started it from.
	const pending = $derived(connectFlow.pending);

	function finish() {
		connectFlow.close();
		void sourcesStore.load();
	}

	// Tauri hands OAuth to the system browser and the SPA stays mounted, so the
	// box finalizes the credential while we're looking at a different app.
	$effect(() => {
		if (!connectFlow.awaitingExternal) return;
		connectFlow.awaitingExternal = false;
		reloadOnReturn(() => void sourcesStore.load());
	});
</script>

<div class="sources-room">
	{#if section !== 'connection' && section !== 'source'}
		<SubNav tabId={tab.id} route={tab.route} base="/sources" default="overview" items={SECTION_ITEMS} divider ariaLabel="Sources sections" />
	{/if}
	{#if section === 'query'}
		<!-- The SQL console fills the room and scrolls itself. -->
		<div class="sources-fill"><DeveloperSqlView {tab} {active} /></div>
	{:else}
	<div class="sources-scroll">
		{#if section === 'catalog'}
			<SourcesCatalog />
		{:else if section === 'activity'}
			<SourcesActivity />
		{:else if section === 'streams'}
			<StreamsView />
		{:else if section === 'connection'}
			<CredentialDetailView {tab} />
		{:else if section === 'source'}
			<SourceDetail sourceId={seg} />
		{:else}
			<SourcesOverview />
		{/if}
	</div>
	{/if}
</div>

<DevicePairModal
	deviceType={pending.kind === 'pair' ? pending.deviceType : 'ios'}
	displayName={pending.kind === 'pair' ? pending.displayName : 'iPhone'}
	open={pending.kind === 'pair'}
	onClose={finish}
	onSuccess={finish}
/>

<ApiKeyConnectModal
	source={pending.kind === 'api_key' ? pending.source : null}
	credentialId={pending.kind === 'api_key' ? pending.credentialId : undefined}
	open={pending.kind === 'api_key'}
	onClose={() => connectFlow.close()}
	onSuccess={finish}
/>

<Modal open={pending.kind === 'chat_import'} onClose={finish} title="Import chat history" width="md">
	<ChatImportCard />
</Modal>

<style>
	.sources-room {
		height: 100%;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}

	.sources-scroll {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
	}

	.sources-fill {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}
</style>
