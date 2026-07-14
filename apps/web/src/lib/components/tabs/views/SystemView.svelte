<!--
	SystemView.svelte

	The System room: live machine overview, 24h history (former Telemetry),
	paired devices, This Mac (desktop only), and the auth activity log — as
	route-driven SubNav sections (/virtues/system, /virtues/system/{section}).
-->

<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { isMacOS } from '$lib/utils/platform';
	import SubNav, { type SubNavItem } from '$lib/components/SubNav.svelte';
	import SystemInfoView from '$lib/components/tabs/views/SystemInfoView.svelte';
	import TelemetryTab from '$lib/components/tabs/views/TelemetryTab.svelte';
	import DevicesView from '$lib/components/tabs/views/DevicesView.svelte';
	import ThisMacView from '$lib/components/tabs/views/ThisMacView.svelte';
	import ActivityView from '$lib/components/tabs/views/ActivityView.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type Section = 'overview' | 'history' | 'devices' | 'this-mac' | 'activity';

	const sections: SubNavItem[] = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'history', label: 'History' },
		{ id: 'devices', label: 'Devices' },
		...(isMacOS ? [{ id: 'this-mac', label: 'This Mac' }] : []),
		{ id: 'activity', label: 'Activity' },
	];

	// Active section is derived from the route (SubNav owns the writing side).
	const section = $derived<Section>(
		(tab.route.match(/^\/virtues\/system\/(history|devices|this-mac|activity)$/)?.[1] as Section) ??
			'overview'
	);

	// Routes from before the consolidation self-heal to their new home.
	const LEGACY_ROUTES: Record<string, string> = {
		'/virtues/telemetry': '/virtues/system/history',
		'/virtues/devices': '/virtues/system/devices',
		'/virtues/this-mac': '/virtues/system/this-mac',
		'/virtues/activity': '/virtues/system/activity',
	};

	$effect(() => {
		const next = LEGACY_ROUTES[tab.route];
		if (next) windowShellStore.updateTab(tab.id, { route: next });
	});
</script>

<div class="system-view">
	<SubNav
		tabId={tab.id}
		route={tab.route}
		base="/virtues/system"
		default="overview"
		items={sections}
		ariaLabel="System sections"
	/>

	<main class="content">
		{#if section === 'overview'}
			<SystemInfoView {tab} {active} />
		{:else if section === 'history'}
			<TelemetryTab {tab} {active} />
		{:else if section === 'devices'}
			<DevicesView {tab} {active} />
		{:else if section === 'this-mac'}
			<ThisMacView {tab} {active} />
		{:else if section === 'activity'}
			<ActivityView {tab} {active} />
		{/if}
	</main>
</div>

<style>
	.system-view {
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
