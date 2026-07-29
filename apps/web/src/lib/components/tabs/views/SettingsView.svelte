<!--
	SettingsView.svelte

	The one Settings room. A single door in the sidebar opens here; every
	preference and console lives under it as a route-driven section. Replaces the
	former two rooms (Account + System) and the flat footer folder
	(Sources · Tools · Profile · Devices · System · Developers · Sign Out).

	Doctrine: one tab group max. The primary sub-nav (the nouns the user owns) is
	the only nav for most sections; only Developer carries a secondary sub-nav,
	and nothing nests a tab group inside a tab group.

	  You          /virtues/you            — profile, theme
	  Assistant    /virtues/assistant      — name, persona, model
	  Sources      /virtues/sources        — connected data sources
	  Billing      /virtues/billing        — plan, wallet, and usage on one page
	  Box          /virtues/box            — box stats / health
	  Devices      /virtues/devices        — paired devices (Unpair lives here)
	  This Mac     /virtues/this-mac       — macOS-only device panel
	  Developer    /virtues/developer      — SQL · Terminal · Lake · Telemetry · Activity

	There is no "Sign out" — auth is the device's proven iroh key, not a server
	session. The destructive action is Unpair, and it lives next to the thing it
	destroys (Devices).

	Legacy flat/nested routes (/virtues/account, /virtues/connections/*,
	/virtues/box/devices, /virtues/developer/console, …) self-heal on mount.
-->

<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { isMacOS } from '$lib/utils/platform';
	import UpdateSection from '$lib/components/settings/UpdateSection.svelte';

	import ProfileView from '$lib/components/tabs/views/ProfileView.svelte';
	import AssistantView from '$lib/components/tabs/views/AssistantView.svelte';
	import ConnectionsPanel from '$lib/components/applets/ConnectionsPanel.svelte';
	import BillingView from '$lib/components/tabs/views/BillingView.svelte';
	import UsageTab from '$lib/components/tabs/views/UsageTab.svelte';
	import SystemInfoView from '$lib/components/tabs/views/SystemInfoView.svelte';
	import DevicesView from '$lib/components/tabs/views/DevicesView.svelte';
	import ThisMacView from '$lib/components/tabs/views/ThisMacView.svelte';
	import DeveloperSqlView from '$lib/components/tabs/views/DeveloperSqlView.svelte';
	import DeveloperTerminalView from '$lib/components/tabs/views/DeveloperTerminalView.svelte';
	import DeveloperLakeView from '$lib/components/tabs/views/DeveloperLakeView.svelte';
	import TelemetryTab from '$lib/components/tabs/views/TelemetryTab.svelte';
	import ActivityView from '$lib/components/tabs/views/ActivityView.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Old routes → their home in the flattened room. Rewritten in place so the
	// sub-nav underline and the content both land on the canonical section.
	const LEGACY_ROUTES: Record<string, string> = {
		'/virtues': '/virtues/you',
		'/virtues/account': '/virtues/you',
		'/virtues/profile': '/virtues/you',
		'/virtues/account/assistant': '/virtues/assistant',
		// Connections → Sources (Tools & apps removed)
		'/virtues/connections': '/virtues/sources',
		'/virtues/connections/sources': '/virtues/sources',
		'/virtues/connections/tools': '/virtues/sources',
		'/virtues/tools': '/virtues/sources',
		// Billing is one page again (plan · wallet · usage)
		'/virtues/account/billing': '/virtues/billing',
		'/virtues/account/usage': '/virtues/billing',
		'/virtues/byo-key': '/virtues/billing',
		'/virtues/usage': '/virtues/billing',
		'/virtues/billing/plan': '/virtues/billing',
		'/virtues/billing/usage': '/virtues/billing',
		// Box → stats; Devices and This Mac are their own tabs now
		'/virtues/system': '/virtues/box',
		'/virtues/box/health': '/virtues/box',
		'/virtues/box/devices': '/virtues/devices',
		'/virtues/system/devices': '/virtues/devices',
		'/virtues/devices/': '/virtues/devices',
		'/virtues/box/this-mac': '/virtues/this-mac',
		'/virtues/system/this-mac': '/virtues/this-mac',
		// Developer flattened — Console layer removed
		'/virtues/developer': '/virtues/developer/sql',
		'/virtues/developer/console': '/virtues/developer/sql',
		'/virtues/system/history': '/virtues/developer/telemetry',
		'/virtues/telemetry': '/virtues/developer/telemetry',
		'/virtues/system/activity': '/virtues/developer/activity',
		'/virtues/activity': '/virtues/developer/activity',
	};

	$effect(() => {
		const next = LEGACY_ROUTES[tab.route];
		if (next) windowShellStore.updateTab(tab.id, { route: next });
	});

	type Section =
		| 'you'
		| 'assistant'
		| 'sources'
		| 'billing'
		| 'box'
		| 'devices'
		| 'this-mac'
		| 'developer';

	const SECTIONS = ['assistant', 'sources', 'billing', 'box', 'devices', 'this-mac', 'developer'];

	// (section, sub) are a pure function of the route. `raw` is everything after
	// `/virtues/`; segment 1 = section, segment 2 = sub-section (Developer only).
	const raw = $derived(tab.route.replace(/^\/virtues\/?/, ''));
	const section = $derived<Section>(
		SECTIONS.includes(raw.split('/')[0]) ? (raw.split('/')[0] as Section) : 'you'
	);
	const sub = $derived(raw.split('/')[1] ?? '');


</script>

<!--
	No SubNav. The sidebar carries this nav now (lib/sidebar/modes.ts) — keeping
	the horizontal row too would mean two navigations for one set of sections,
	side by side, disagreeing about which is in charge. Removing it is also what
	retires the second underline row Developer used to add, which was the
	original complaint.
-->
<div class="settings-view">
	<main class="content">
		{#if section === 'you'}
			<ProfileView {tab} {active} />
		{:else if section === 'assistant'}
			<AssistantView {tab} {active} />
		{:else if section === 'sources'}
			<!-- ConnectionsPanel is self-driven (no tab/active props). -->
			<ConnectionsPanel />
		{:else if section === 'billing'}
			<!-- Plan · wallet · usage on one scrolling page. Each view is a
			     full-height <Page>; neutralize that so .content is the one scroller. -->
			<div class="billing-stack">
				<BillingView {tab} {active} />
				<UsageTab {tab} {active} />
			</div>
		{:else if section === 'box'}
			<UpdateSection />
			<SystemInfoView {tab} {active} />
		{:else if section === 'devices'}
			<DevicesView {tab} {active} />
		{:else if section === 'this-mac' && isMacOS}
			<ThisMacView {tab} {active} />
		{:else if section === 'developer'}
			{#if sub === 'terminal'}
				<DeveloperTerminalView {tab} {active} />
			{:else if sub === 'lake'}
				<DeveloperLakeView {tab} {active} />
			{:else if sub === 'telemetry'}
				<TelemetryTab {tab} {active} />
			{:else if sub === 'activity'}
				<ActivityView {tab} {active} />
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

	/* Billing stacks two full-height <Page>s; let them size to content so the
	   parent .content scrolls once instead of nesting two scroll regions. */
	.billing-stack :global(.page-container) {
		height: auto;
		overflow: visible;
	}
</style>
