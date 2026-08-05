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
	  Billing      /virtues/billing        — plan and payment method
	  Usage        /virtues/usage          — AI-call log, background runs, system
	  Box          /virtues/box            — box stats / health
	  Devices      /virtues/devices        — paired devices (Unpair lives here)
	  This Mac     /virtues/this-mac       — macOS-only device panel
	  Developer    /virtues/developer      — SQL · Terminal · Lake

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
	import BillingView from '$lib/components/tabs/views/BillingView.svelte';
	import UsageView from '$lib/components/tabs/views/UsageView.svelte';
	import SystemInfoView from '$lib/components/tabs/views/SystemInfoView.svelte';
	import DevicesView from '$lib/components/tabs/views/DevicesView.svelte';
	import ThisMacView from '$lib/components/tabs/views/ThisMacView.svelte';
	import DeveloperSqlView from '$lib/components/tabs/views/DeveloperSqlView.svelte';
	import DeveloperTerminalView from '$lib/components/tabs/views/DeveloperTerminalView.svelte';
	import DeveloperLakeView from '$lib/components/tabs/views/DeveloperLakeView.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Old routes → their home in the flattened room. Rewritten in place so the
	// sub-nav underline and the content both land on the canonical section.
	const LEGACY_ROUTES: Record<string, string> = {
		'/virtues': '/virtues/you',
		'/virtues/account': '/virtues/you',
		'/virtues/profile': '/virtues/you',
		'/virtues/account/assistant': '/virtues/assistant',
		// Billing is plan + payment method. The usage panel that used to sit
		// under it never loaded; Usage is now its own section, built from the
		// box-local call log (the numbers that do load).
		'/virtues/account/billing': '/virtues/billing',
		'/virtues/byo-key': '/virtues/billing',
		'/virtues/billing/plan': '/virtues/billing',
		'/virtues/account/usage': '/virtues/usage',
		'/virtues/billing/usage': '/virtues/usage',
		// Telemetry was this page under a word for something you send somewhere.
		'/virtues/telemetry': '/virtues/usage',
		'/virtues/developer/telemetry': '/virtues/usage',
		'/virtues/system/history': '/virtues/usage',
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
		'/virtues/connections',
		'/virtues/connections/sources',
		'/virtues/connections/tools',
		'/virtues/tools',
	];

	$effect(() => {
		if (SOURCES_ALIASES.includes(tab.route)) {
			windowShellStore.navigate('/sources', { label: 'Sources' });
			return;
		}
		const next = LEGACY_ROUTES[tab.route];
		if (next) windowShellStore.updateTab(tab.id, { route: next });
	});

	type Section =
		| 'you'
		| 'assistant'
		| 'billing'
		| 'usage'
		| 'box'
		| 'devices'
		| 'this-mac'
		| 'developer';

	const SECTIONS = [
		'assistant',
		'billing',
		'usage',
		'box',
		'devices',
		'this-mac',
		'developer',
	];

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
		{:else if section === 'billing'}
			<BillingView {tab} {active} />
		{:else if section === 'usage'}
			<UsageView {tab} {active} />
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
</style>
