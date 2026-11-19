<!--
	SettingsView.svelte

	The one Settings room. Bare /virtues is the landing page
	(settings/SettingsLanding.svelte): search, what needs you, Changed
	lately, and a link to each page. Each section of lib/settings is a page
	(settings/SettingsSection.svelte); the other routes are pages a row
	opens, for what is a list, a chart, a table or a long form.

	  (landing)    /virtues                — search, Changed lately, every page
	  Profile      /virtues/profile        — you: name, birthday, work, home
	  How it talks /virtues/assistant      — its name, its manner, what it never raises
	  Can do       /virtues/assistant/tools — its tools, and suggested questions
	  Spending     /virtues/assistant/spending — models, and what tasks may spend
	               /virtues/assistant/spending/models — the model list (?role= picks one)
	               /virtues/assistant/spending/cost   — what AI has cost, and every call
	               /virtues/assistant/spending/provider — your own AI provider
	  Appearance   /virtues/appearance     — theme, reading font, motion
	  Billing      /virtues/billing        — subscription, balance, auto top-up
	  Your data    /virtues/data           — backup, export, keep recordings, deleting
	               /virtues/data/shared    — every link shared through the door
	               /virtues/data/keys      — the named keys applets use
	  Your server  /virtues/system         — whether it is all right, what it runs, the machine
	  Updates      /virtues/updates        — new releases, and which ones it takes
	  Backup       /virtues/backup         — the newest backup, the drives, restoring
	  Search       /virtues/search         — where search runs, and what it covers
	  Devices      /virtues/devices        — paired devices, reaching from away, Start over
	  This device  /virtues/devices/this   — the device you're using (iOS: its own panel)
	               /virtues/devices/:id    — another device, as the box knows it
	  Applets      /virtues/applets        — every applet, built-in ones included
	  Terminal     /virtues/developer/terminal — a shell on your server, opened from ⌘K

	There is no "Sign out": auth is the device's proven iroh key, not a server
	session. The destructive actions are Unpair, on the page of the device it
	removes, and Start over, at the foot of Devices, which unpairs everything.

	Legacy flat/nested routes (/virtues/account, /virtues/connections/*,
	/virtues/box/devices, /virtues/developer/console, …) self-heal on mount.
-->

<script lang="ts">
	import { usePageWidth } from '$lib/stores/pageWidth.svelte';
	import type { Tab } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { isMacOS, isIOS } from '$lib/utils/platform';

	import SettingsLanding from '$lib/components/settings/SettingsLanding.svelte';
	import SettingsSection from '$lib/components/settings/SettingsSection.svelte';
	import ByoPage from '$lib/components/settings/ByoPage.svelte';
	import BackupPage from '$lib/components/settings/BackupPage.svelte';
	import UpdatesPage from '$lib/components/settings/UpdatesPage.svelte';
	import ModelCatalogView from '$lib/components/tabs/views/ModelCatalogView.svelte';
	import BillingView from '$lib/components/tabs/views/BillingView.svelte';
	import SystemInfoView from '$lib/components/tabs/views/SystemInfoView.svelte';
	import DevicesView from '$lib/components/tabs/views/DevicesView.svelte';
	import DeviceView from '$lib/components/tabs/views/DeviceView.svelte';
	import ThisDeviceView from '$lib/components/tabs/views/ThisDeviceView.svelte';
	import DeveloperTerminalView from '$lib/components/tabs/views/DeveloperTerminalView.svelte';
	import SharedLinksView from '$lib/components/tabs/views/SharedLinksView.svelte';
	import KeysView from '$lib/components/tabs/views/KeysView.svelte';
	import ManageApplets from '$lib/components/applets/ManageApplets.svelte';
	import SearchSettingsView from '$lib/components/tabs/views/SearchSettingsView.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Old routes → where each lives now. Rewritten in place, so the tab's
	// history keeps only the current route.
	const LEGACY_ROUTES: Record<string, string> = {
		'/virtues/you': '/virtues/profile',
		'/virtues/account': '/virtues/profile',
		'/virtues/account/assistant': '/virtues/assistant',
		// Memory is retired: Virtues learns you from your record, not from notes.
		'/virtues/assistant/memory': '/virtues/assistant',
		// The model list and the cost page sit under Models and spending.
		'/virtues/models': '/virtues/assistant/spending/models',
		'/virtues/assistant/models': '/virtues/assistant/spending/models',
		'/virtues/billing/cost': '/virtues/assistant/spending/cost',
		// Billing and Usage merged into Plan (2026-08-31): one subject seen from
		// two ends — what you are on, what is left, what drew it down. Both of
		// their own doors redirect too, because they were live sections with
		// bookmarks, not just historical paths.
		'/virtues/usage': '/virtues/assistant/spending/cost',
		'/virtues/account/billing': '/virtues/billing',
		// Your own AI provider is where it runs, so it sits under Models and spending.
		'/virtues/byo-key': '/virtues/assistant/spending/provider',
		'/virtues/billing/byo': '/virtues/assistant/spending/provider',
		'/virtues/billing/plan': '/virtues/billing',
		'/virtues/account/usage': '/virtues/assistant/spending/cost',
		'/virtues/billing/usage': '/virtues/assistant/spending/cost',
		// Telemetry was the cost page under a word for something you send somewhere.
		'/virtues/telemetry': '/virtues/assistant/spending/cost',
		'/virtues/developer/telemetry': '/virtues/assistant/spending/cost',
		// "Plan" named a choice that does not exist — one subscription, one
		// price. Short-lived (2026-08-31 → 09-03), but it was a live row with
		// bookmarks, so its door stays open.
		'/virtues/plan': '/virtues/billing',
		// The 24h readings went back to System (2026-09-03): they are the
		// machine measured, and had no business under a wallet balance.
		'/virtues/system/history': '/virtues/system',
		// Updates holds the versions each part runs; reaching the server from
		// away is on Devices; the machine itself is Your server.
		'/virtues/server': '/virtues/updates',
		'/virtues/network': '/virtues/devices',
		'/virtues/software': '/virtues/updates',
		'/virtues/devices/server': '/virtues/updates',
		'/virtues/box': '/virtues/system',
		'/virtues/box/health': '/virtues/system',
		'/virtues/box/network': '/virtues/devices',
		'/virtues/box/updates': '/virtues/updates',
		'/virtues/version': '/virtues/updates',
		// This Mac is `devices/this`, a device page like any other, and a
		// stable word Sources can link to without knowing a device id.
		'/virtues/box/devices': '/virtues/devices',
		'/virtues/system/devices': '/virtues/devices',
		'/virtues/devices/': '/virtues/devices',
		'/virtues/this-mac': '/virtues/devices/this',
		'/virtues/box/this-mac': '/virtues/devices/this',
		'/virtues/system/this-mac': '/virtues/devices/this',
		'/virtues/terminal': '/virtues/developer/terminal',
		// The auth-activity log is gone. Its old doors land on Devices, which is
		// where the thing it reported on — what is paired, and what you can
		// revoke — actually lives.
		'/virtues/system/activity': '/virtues/devices',
		'/virtues/activity': '/virtues/devices',
		'/virtues/developer/activity': '/virtues/devices',
		// Shared links and Keys open from Your data.
		'/virtues/shared': '/virtues/data/shared',
		'/virtues/keys': '/virtues/data/keys',
	};

	// Sources left Settings for its own door. `/sources` is a different tab
	// type, so these go through `retarget`, which re-resolves the type and
	// replaces this tab's history slot in place.
	const SOURCES_ALIASES = [
		'/virtues/sources',
		'/virtues/connections/sources',
		'/virtues/connections/tools',
		'/virtues/tools',
	];

	// Streams and SQL are sections of Sources now (Streams, Query). Their old
	// doors land there through `retarget`, like Sources' aliases. Terminal
	// keeps its route; ⌘K opens it.
	const STREAMS_ALIASES = ['/virtues/lake', '/virtues/developer/lake'];
	const QUERY_ALIASES = ['/virtues/developer', '/virtues/developer/sql', '/virtues/developer/console', '/virtues/sql'];

	$effect(() => {
		// Services the assistant can act in are rows in the Sources catalog.
		if (tab.route === '/virtues/connections') {
			windowShellStore.retarget(tab.id, '/sources/catalog', 'Sources · Catalog');
			return;
		}
		if (SOURCES_ALIASES.includes(tab.route)) {
			windowShellStore.retarget(tab.id, '/sources', 'Sources');
			return;
		}
		if (STREAMS_ALIASES.includes(tab.route)) {
			windowShellStore.retarget(tab.id, '/sources/streams', 'Sources · Streams');
			return;
		}
		if (QUERY_ALIASES.includes(tab.route)) {
			windowShellStore.retarget(tab.id, '/sources/query', 'Sources · Query');
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
		| 'profile'
		| 'appearance'
		| 'updates'
		| 'data'
		| 'backup'
		| 'assistant'
		| 'billing'
		| 'system'
		| 'search'
		| 'devices'
		| 'applets'
		| 'developer';

	const SECTIONS = [
		'profile',
		'appearance',
		'updates',
		'data',
		'backup',
		'assistant',
		'billing',
		'system',
		'search',
		'devices',
		'applets',
		'developer',
	];

	// (section, sub) are a pure function of the route. `raw` is everything after
	// `/virtues/`; segment 1 = section, segment 2 = sub-section (Assistant, Devices, Developer).
	// The query (`?role=` on the model list) is not part of the path.
	const routePath = $derived(tab.route.split('?')[0]);
	const routeQuery = $derived(tab.route.split('?')[1] ?? '');
	const raw = $derived(routePath.replace(/^\/virtues\/?/, ''));
	const section = $derived<Section>(
		SECTIONS.includes(raw.split('/')[0]) ? (raw.split('/')[0] as Section) : 'page'
	);

	// The applets section lays out its own column. The other sections are
	// SettingsPage's, which sets its own width.
	const width = usePageWidth(() => (section === 'applets' ? 'full' : null));
	const sub = $derived(raw.split('/')[1] ?? '');
	const subPage = $derived(raw.split('/')[2] ?? '');
	const role = $derived(new URLSearchParams(routeQuery).get('role') ?? undefined);

	// No section list sits beside these pages: the landing links to each,
	// and each page links back to it.
</script>

<div class="settings-view">
	<main class="content">
		{#if section === 'page'}
			<SettingsLanding />
		{:else if section === 'profile'}
			<SettingsSection section="you" />
		{:else if section === 'appearance'}
			<SettingsSection section="appearance" />
		{:else if section === 'updates'}
			<UpdatesPage />
		{:else if section === 'data'}
			{#if sub === 'shared'}
				<SharedLinksView />
			{:else if sub === 'keys'}
				<KeysView />
			{:else}
				<SettingsSection section="data" />
			{/if}
		{:else if section === 'backup'}
			<BackupPage />
		{:else if section === 'assistant'}
			{#if sub === 'tools'}
				<SettingsSection section="tools" />
			{:else if sub === 'spending' && subPage === 'models'}
				<ModelCatalogView {role} />
			{:else if sub === 'spending' && subPage === 'cost'}
				<BillingView />
			{:else if sub === 'spending' && subPage === 'provider'}
				<ByoPage />
			{:else if sub === 'spending'}
				<SettingsSection section="spending" />
			{:else}
				<SettingsSection section="assistant" />
			{/if}
		{:else if section === 'billing'}
			<SettingsSection section="billing" />
		{:else if section === 'system'}
			<SystemInfoView {tab} {active} />
		{:else if section === 'search'}
			<SearchSettingsView />
		{:else if section === 'applets'}
			<!-- Padding on the scroller, the page's width inside it: the same
			     box model as the Applets page in Home, so the two line up. -->
			<div class="applets-settings">
				<div class="measure" style:max-width={width.css}>
					<ManageApplets {tab} />
				</div>
			</div>
		{:else if section === 'devices'}
			{#if !sub}
				<DevicesView {tab} {active} />
			{:else if sub === 'this' && isIOS}
				<!-- The phone's own page reads plugins only the phone has. iOS
				     first, because an iPad satisfies `isMacOS` too (it reports a
				     desktop platform string; see utils/platform.ts). -->
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
		{:else if section === 'developer' && sub === 'terminal'}
			<!-- Terminal, opened from ⌘K. SQL is Sources · Query; the other
			     developer routes forward there, so they mount nothing here. -->
			<DeveloperTerminalView {tab} {active} />
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
		width: 100%;
		margin: 0 auto;
	}
</style>
