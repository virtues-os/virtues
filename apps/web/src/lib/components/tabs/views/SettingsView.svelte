<!--
	SettingsView.svelte

	The one Settings room. Bare /virtues is Settings on one page
	(settings/SettingsPage.svelte): every setting you change, in six sections
	from lib/settings. The routes below are the pages a row on it opens, for
	what is a list, a chart, a table or a long form.

	  (page)       /virtues                — every setting, six sections
	  Memory       /virtues/assistant/memory — what the assistant remembers
	  Models       /virtues/assistant/models — the catalog
	  Cost         /virtues/billing        — what AI has cost, and every call
	  Your own AI  /virtues/billing/byo    — bringing your own AI
	  Keys         /virtues/keys           — sign-ins and the named keys applets use
	  System       /virtues/system         — the machine, measured (read-only)
	  Search       /virtues/search         — where search runs, and what it covers
	  Network      /virtues/network        — which network the box is on
	  Software     /virtues/software       — release, track, update, artifacts
	  Devices      /virtues/devices        — paired devices (Unpair, Start over)
	               /virtues/devices/this   — the machine you're on (local panel)
	               /virtues/devices/:id    — another device, as the box knows it
	  Shared links /virtues/shared         — every link shared through the door
	  Applets      /virtues/applets        — every applet, built-in ones included
	  Developer    /virtues/developer      — SQL · Terminal

	"Box" was three of those in one door (2026-08-17). It stacked a Wi-Fi
	picker, an update installer, an 8-chapter telemetry console and a
	revoke-every-device button on a single scroll — a reading, two settings and
	a detonator sharing one scrollbar — and the console's own "Network &
	Devices" and "About" chapters answered questions the two sections above
	them had already answered differently. Splitting it by subject is what
	removes the duplication; it was never really one room.

	There is no "Sign out" — auth is the device's proven iroh key, not a server
	session. The destructive action is Unpair, and it lives next to the thing it
	destroys (Devices) — which is also why "Start over", the plural of Unpair,
	moved there out of Box.

	Legacy flat/nested routes (/virtues/account, /virtues/connections/*,
	/virtues/box/devices, /virtues/developer/console, …) self-heal on mount.
-->

<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { isMacOS, isIOS } from '$lib/utils/platform';
	import SoftwareView from '$lib/components/tabs/views/SoftwareView.svelte';

	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import MemoryPage from '$lib/components/settings/MemoryPage.svelte';
	import ByoPage from '$lib/components/settings/ByoPage.svelte';
	import ModelCatalogView from '$lib/components/tabs/views/ModelCatalogView.svelte';
	import BillingView from '$lib/components/tabs/views/BillingView.svelte';
	import SystemInfoView from '$lib/components/tabs/views/SystemInfoView.svelte';
	import DevicesView from '$lib/components/tabs/views/DevicesView.svelte';
	import DeviceView from '$lib/components/tabs/views/DeviceView.svelte';
	import ThisDeviceView from '$lib/components/tabs/views/ThisDeviceView.svelte';
	import DeveloperSqlView from '$lib/components/tabs/views/DeveloperSqlView.svelte';
	import DeveloperTerminalView from '$lib/components/tabs/views/DeveloperTerminalView.svelte';
	import SharedLinksView from '$lib/components/tabs/views/SharedLinksView.svelte';
	import KeysView from '$lib/components/tabs/views/KeysView.svelte';
	import ManageApplets from '$lib/components/applets/ManageApplets.svelte';
	import SearchSettingsView from '$lib/components/tabs/views/SearchSettingsView.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Old routes → their home in the flattened room. Rewritten in place so the
	// sub-nav underline and the content both land on the canonical section.
	const LEGACY_ROUTES: Record<string, string> = {
		// You and Assistant were pages of their own until Settings became one
		// page (2026-11-17).
		'/virtues/you': '/virtues',
		'/virtues/account': '/virtues',
		'/virtues/profile': '/virtues',
		'/virtues/assistant': '/virtues',
		'/virtues/account/assistant': '/virtues',
		// The model catalog is Assistant's sub-page, beside the pickers it serves.
		'/virtues/models': '/virtues/assistant/models',
		// Billing and Usage merged into Plan (2026-08-31): one subject seen from
		// two ends — what you are on, what is left, what drew it down. Both of
		// their own doors redirect too, because they were live sections with
		// bookmarks, not just historical paths.
		'/virtues/usage': '/virtues/billing',
		'/virtues/account/billing': '/virtues/billing',
		'/virtues/byo-key': '/virtues/billing/byo',
		'/virtues/billing/plan': '/virtues/billing',
		'/virtues/account/usage': '/virtues/billing',
		'/virtues/billing/usage': '/virtues/billing',
		// Telemetry was this page under a word for something you send somewhere.
		'/virtues/telemetry': '/virtues/billing',
		'/virtues/developer/telemetry': '/virtues/billing',
		// "Plan" named a choice that does not exist — one subscription, one
		// price. Short-lived (2026-08-31 → 09-03), but it was a live row with
		// bookmarks, so its door stays open.
		'/virtues/plan': '/virtues/billing',
		// The 24h readings went back to System (2026-09-03): they are the
		// machine measured, and had no business under a wallet balance.
		'/virtues/system/history': '/virtues/system',
		// Network and updates are rows on the Settings page; versions are the
		// server's device page; the machine measured is System.
		'/virtues/network': '/virtues',
		'/virtues/software': '/virtues/devices/server',
		'/virtues/box': '/virtues/system',
		'/virtues/box/health': '/virtues/system',
		'/virtues/box/network': '/virtues',
		'/virtues/box/updates': '/virtues',
		'/virtues/updates': '/virtues',
		'/virtues/version': '/virtues/devices/server',
		// This Mac is `devices/this`, a device page like any other, and a
		// stable word Sources can link to without knowing a device id.
		'/virtues/box/devices': '/virtues/devices',
		'/virtues/system/devices': '/virtues/devices',
		'/virtues/devices/': '/virtues/devices',
		'/virtues/this-mac': '/virtues/devices/this',
		'/virtues/box/this-mac': '/virtues/devices/this',
		'/virtues/system/this-mac': '/virtues/devices/this',
		// Developer flattened — Console layer removed
		'/virtues/developer': '/virtues/developer/sql',
		'/virtues/developer/console': '/virtues/developer/sql',
		// The pre-Settings developer doors (/virtues/sql, and the /developers/*
		// group that used to self-heal them). One room now.
		'/virtues/sql': '/virtues/developer/sql',
		'/virtues/terminal': '/virtues/developer/terminal',
		// The auth-activity log is gone. Its old doors land on Devices, which is
		// where the thing it reported on — what is paired, and what you can
		// revoke — actually lives.
		'/virtues/system/activity': '/virtues/devices',
		'/virtues/activity': '/virtues/devices',
		'/virtues/developer/activity': '/virtues/devices',
	};

	// Sources left Settings for its own door, so these can't be rewritten in
	// place the way the others are — `/sources` is a different tab type, and
	// only `navigate` re-resolves that through the registry.
	const SOURCES_ALIASES = [
		'/virtues/sources',
		'/virtues/connections/sources',
		'/virtues/connections/tools',
		'/virtues/tools',
	];

	// The Lake console is gone: it listed the archive's streams, which Drive's
	// Streams page shows. Its doors land there, a different tab type, so they
	// go through `navigate` like Sources' aliases.
	const LAKE_ALIASES = ['/virtues/lake', '/virtues/developer/lake'];

	$effect(() => {
		// Services the assistant can act in are rows in the Sources catalog.
		if (tab.route === '/virtues/connections') {
			windowShellStore.navigate('/sources/catalog', { label: 'Sources · Catalog' });
			return;
		}
		if (SOURCES_ALIASES.includes(tab.route)) {
			windowShellStore.navigate('/sources', { label: 'Sources' });
			return;
		}
		if (LAKE_ALIASES.includes(tab.route)) {
			windowShellStore.navigate('/storage/streams', { label: 'Streams' });
			return;
		}
		const next = LEGACY_ROUTES[tab.route];
		// Never write a route that is already there. This effect READS
		// `tab.route` and WRITES it, so it is only safe while every write
		// changes the value — the same shape that took the whole window's
		// reactivity down when a chat titled "New Chat" made ChatView's label
		// effect rewrite its own value forever. A legacy mapping that ever
		// resolved to its own key would do it again; this makes that
		// impossible rather than relying on the table staying acyclic.
		if (next && next !== tab.route) windowShellStore.updateTab(tab.id, { route: next });
	});

	type Section =
		| 'page'
		| 'assistant'
		| 'billing'
		| 'keys'
		| 'system'
		| 'search'
		| 'devices'
		| 'shared'
		| 'applets'
		| 'developer';

	const SECTIONS = [
		'assistant',
		'billing',
		'keys',
		'system',
		'search',
		'devices',
		'shared',
		'applets',
		'developer',
	];

	// (section, sub) are a pure function of the route. `raw` is everything after
	// `/virtues/`; segment 1 = section, segment 2 = sub-section (Assistant, Devices, Developer).
	const raw = $derived(tab.route.replace(/^\/virtues\/?/, ''));
	const section = $derived<Section>(
		SECTIONS.includes(raw.split('/')[0]) ? (raw.split('/')[0] as Section) : 'page'
	);
	const sub = $derived(raw.split('/')[1] ?? '');

	// No section list sits beside these pages. Bare /virtues is the one page
	// with every setting, and its section names scroll within it; the routes
	// above are pages its rows open, and Back returns to it.
</script>

<div class="settings-view">
	<main class="content">
		{#if section === 'page'}
			<SettingsPage />
		{:else if section === 'assistant'}
			{#if sub === 'memory'}
				<MemoryPage />
			{:else}
				<ModelCatalogView />
			{/if}
		{:else if section === 'billing'}
			{#if sub === 'byo'}
				<ByoPage />
			{:else}
				<BillingView />
			{/if}
		{:else if section === 'keys'}
			<KeysView />
		{:else if section === 'system'}
			<SystemInfoView {tab} {active} />
		{:else if section === 'search'}
			<SearchSettingsView />
		{:else if section === 'shared'}
			<SharedLinksView />
		{:else if section === 'applets'}
			<!-- Padding on the scroller, the 72rem measure inside it: the same
			     box model as the Applets page in Home, so the two line up. -->
			<div class="applets-settings">
				<div class="measure">
					<ManageApplets {tab} />
				</div>
			</div>
		{:else if section === 'devices'}
			{#if !sub}
				<DevicesView {tab} {active} />
			{:else if sub === 'server'}
				<!-- The server's own page. It is row zero of Devices, not a
				     separate subject: "what release does this machine run" is a
				     fact about a device. -->
				<SoftwareView />
			{:else if sub === 'this' && isIOS}
				<!-- iOS keeps its own instrument panel for now: MobileDeviceScreen
				     is 900 lines of stream-by-stream local detail that has no
				     box-side equivalent. iOS FIRST, because an iPad satisfies
				     `isMacOS` too (it reports a desktop platform string; see
				     utils/platform.ts) and would otherwise get the Mac panel. -->
				<ThisDeviceView />
			{:else}
				<!-- ONE page for every other device, including this Mac. It used
				     to be three, split by vantage — what the box knows vs what the
				     device knows — with nothing on screen saying that was the axis,
				     so permissions and versions appeared twice and disagreed for
				     reasons no reader could see. DeviceView states the vantage per
				     section instead, and reads the local daemon when it is
				     standing on the machine it describes. -->
				<DeviceView deviceId={sub} {tab} {active} />
			{/if}
		{:else if section === 'developer'}
			{#if sub === 'terminal'}
				<DeveloperTerminalView {tab} {active} />
			{:else}
				<DeveloperSqlView {tab} {active} />
			{/if}
		{/if}
	</main>
</div>

<style>
	.settings-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.content {
		flex: 1;
		overflow-y: auto;
		min-height: 0;
	}

	.applets-settings {
		padding: 48px;
	}

	.measure {
		max-width: 72rem;
		width: 100%;
		margin: 0 auto;
	}
</style>
