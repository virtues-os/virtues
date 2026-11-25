<!--
	SourcesView: the Sources room, opened from Settings' Server line
	(LANDING in lib/settings) and from ⌘K.

	`/sources` is one page (SourcesPage) and `/sources/<id>` one source
	(SourceDetail), reached from it. A connection has no page of its own:
	`/sources/cred_…` forwards to its source once the store knows it. The words in SOURCES_SECTIONS were sections of this room; they now
	only forward: Catalog and Activity to the page itself, Streams and Query to
	Settings › Your data.
-->
<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { SOURCES_SECTIONS } from '$lib/tabs/registry';
	import ConnectWindow from './ConnectWindow.svelte';
	import DevicePairModal from './DevicePairModal.svelte';
	import ApiKeyConnectModal from './ApiKeyConnectModal.svelte';
	import SourcesPage from './SourcesPage.svelte';
	import SourceDetail from './SourceDetail.svelte';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { sourcesStore } from '$lib/stores/sources.svelte';

	let { tab }: { tab: Tab; active?: boolean } = $props();

	/** Where each old section word goes now. */
	const FORWARDS: Record<string, { route: string; label: string }> = {
		catalog: { route: '/sources', label: 'Sources' },
		activity: { route: '/sources', label: 'Sources' },
		streams: { route: '/virtues/data/streams', label: 'Settings · Streams' },
		query: { route: '/virtues/data/query', label: 'Settings · Query' }
	};

	// Three shapes share this slot, discriminated without a lookup:
	//   ''                  → the page
	//   a reserved word     → forwarded (see FORWARDS)
	//   `cred_…`            → one connection
	//   anything else       → one source (catalog ids are bare slugs)
	// Keying the connection case on the id prefix rather than on "is this in the
	// catalog" keeps the dispatch synchronous — a source that is no longer
	// installed still needs a page, precisely so its leftover connections can be
	// found and removed.
	const seg = $derived(tab.route.split('?')[0].replace(/^\/sources\/?/, '').split('/')[0]);

	const section = $derived.by(() => {
		if (seg === '') return 'page';
		if ((SOURCES_SECTIONS as readonly string[]).includes(seg)) return 'forward';
		return seg.startsWith('cred_') ? 'connection' : 'source';
	});

	$effect(() => {
		if (section !== 'forward') return;
		const to = FORWARDS[seg];
		if (to) windowShellStore.retarget(tab.id, to.route, to.label);
	});

	$effect(() => {
		if (section !== 'connection') return;
		if (sourcesStore.loading) {
			void sourcesStore.load();
			return;
		}
		const c = sourcesStore.connections.find((x) => x.id === seg);
		if (c) windowShellStore.retarget(tab.id, `/sources/${c.sourceId}`, sourcesStore.sourceLabel(c.sourceId));
		else windowShellStore.retarget(tab.id, '/sources', 'Sources');
	});

	// The connect windows live here rather than in a page so that finishing a
	// connect doesn't depend on which page you started it from.
	const pending = $derived(connectFlow.pending);

	function finish() {
		connectFlow.close();
		void sourcesStore.load();
	}
</script>

<div class="sources-room">
	<div class="sources-scroll">
		{#if section === 'source'}
			<SourceDetail sourceId={seg} tabId={tab.id} />
		{:else if section === 'page'}
			<SourcesPage {tab} />
		{/if}
	</div>
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

<ConnectWindow />

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
</style>
