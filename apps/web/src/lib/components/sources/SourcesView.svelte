<!--
	SourcesView — the Sources room.

	Sources used to be one row inside Settings rendering a single panel: a
	paragraph explaining what a source is, a flow table, and a flat grid of
	credentials. Two halves that shared no join key (the flow table is keyed on
	ontology tables, the grid on credentials), so a stopped stream could not be
	clicked through to whatever was supposed to be filling it.

	Now it is a door with three sections, in the order the questions get asked:

	  Overview  — is anything broken right now, and is data still arriving
	  Catalog   — what can I plug in, and what is already plugged in
	  Activity  — what has actually been running

	Section is a pure function of the route, the way SettingsView does it, and
	the reserved words live in the registry beside the matcher (SOURCES_SECTIONS)
	so the router and this file cannot disagree about whether `/sources/catalog`
	is a section or a credential id.

	No SubNav: the sidebar mode carries this nav (lib/sidebar/modes.ts), and a
	horizontal strip saying the same thing would be two navigations for one set
	of sections.
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
	import CredentialDetailView from '$lib/components/tabs/views/CredentialDetailView.svelte';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { reloadOnReturn } from './connectDispatch';

	let { tab }: { tab: Tab; active?: boolean } = $props();

	// A reserved word is a section; anything else in that slot is a connection id.
	const seg = $derived(tab.route.replace(/^\/sources\/?/, '').split('/')[0]);
	const section = $derived(
		seg === '' ? 'overview' : (SOURCES_SECTIONS as readonly string[]).includes(seg) ? seg : 'detail'
	);

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
	{#if section === 'catalog'}
		<SourcesCatalog />
	{:else if section === 'activity'}
		<SourcesActivity />
	{:else if section === 'detail'}
		<CredentialDetailView {tab} />
	{:else}
		<SourcesOverview />
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
		overflow-y: auto;
	}
</style>
