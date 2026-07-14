<!--
	AccountView.svelte

	The Account room: Profile, Assistant, Billing, and Usage as route-driven
	SubNav sections (/virtues/account, /virtues/account/{assistant|billing|usage}).
	Absorbs the former standalone /virtues/{assistant,billing,usage,byo-key} pages
	(BYO key management now lives inside Billing).
-->

<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import SubNav, { type SubNavItem } from '$lib/components/SubNav.svelte';
	import ProfileView from '$lib/components/tabs/views/ProfileView.svelte';
	import AssistantView from '$lib/components/tabs/views/AssistantView.svelte';
	import BillingView from '$lib/components/tabs/views/BillingView.svelte';
	import UsageTab from '$lib/components/tabs/views/UsageTab.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type Section = 'profile' | 'assistant' | 'billing' | 'usage';

	const sections: SubNavItem[] = [
		{ id: 'profile', label: 'Profile' },
		{ id: 'assistant', label: 'Assistant' },
		{ id: 'billing', label: 'Billing' },
		{ id: 'usage', label: 'Usage' },
	];

	// Active section is derived from the route (SubNav owns the writing side).
	const section = $derived<Section>(
		(tab.route.match(/^\/virtues\/account\/(assistant|billing|usage)$/)?.[1] as Section) ??
			'profile'
	);

	// Routes from before the consolidation self-heal to their new home.
	const LEGACY_ROUTES: Record<string, string> = {
		'/virtues/assistant': '/virtues/account/assistant',
		'/virtues/billing': '/virtues/account/billing',
		'/virtues/usage': '/virtues/account/usage',
		'/virtues/byo-key': '/virtues/account/billing',
	};

	$effect(() => {
		const next = LEGACY_ROUTES[tab.route];
		if (next) windowShellStore.updateTab(tab.id, { route: next });
	});
</script>

<div class="account-view">
	<SubNav
		tabId={tab.id}
		route={tab.route}
		base="/virtues/account"
		default="profile"
		items={sections}
		ariaLabel="Account sections"
	/>

	<main class="content">
		{#if section === 'profile'}
			<ProfileView {tab} {active} />
		{:else if section === 'assistant'}
			<AssistantView {tab} {active} />
		{:else if section === 'billing'}
			<BillingView {tab} {active} />
		{:else if section === 'usage'}
			<UsageTab {tab} {active} />
		{/if}
	</main>
</div>

<style>
	.account-view {
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
