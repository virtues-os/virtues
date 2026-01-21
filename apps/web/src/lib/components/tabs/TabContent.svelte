<script lang="ts">
	import type { Tab } from '$lib/stores/windowTabs.svelte';
	import ChatView from './views/ChatView.svelte';
	import HistoryView from './views/HistoryView.svelte';
	import ContextView from './views/ContextView.svelte';
	import WikiView from './views/WikiView.svelte';
	import WikiDetailView from './views/WikiDetailView.svelte';
	import WikiListView from './views/WikiListView.svelte';
	import DataSourcesView from './views/DataSourcesView.svelte';
	import UsageView from './views/UsageView.svelte';
	import StorageView from './views/StorageView.svelte';
	import JobsView from './views/JobsView.svelte';
	import ProfileView from './views/ProfileView.svelte';
	import EntitiesView from './views/EntitiesView.svelte';
	import DriveView from './views/DriveView.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Check if this is a wiki overview (no slug) or wiki detail page (has slug)
	const isWikiOverview = $derived(tab.type === 'wiki' && !tab.slug);
	const isWikiDetail = $derived(tab.type === 'wiki' && !!tab.slug);

	// Get display info for placeholder views (for types not yet implemented)
	function getPlaceholderInfo(tab: Tab): { icon: string; title: string; subtitle?: string } {
		switch (tab.type) {
			case 'data-sources-add':
				return {
					icon: 'ri:add-circle-line',
					title: 'Add Data Source',
					subtitle: 'Connect a new data source'
				};
			default:
				return { icon: 'ri:file-line', title: 'Page', subtitle: tab.route };
		}
	}

	const placeholderInfo = $derived(getPlaceholderInfo(tab));
</script>

<div class="tab-content" class:active style:display={active ? 'flex' : 'none'}>
	{#if tab.type === 'chat'}
		<ChatView {tab} {active} />
	{:else if tab.type === 'history'}
		<HistoryView {tab} {active} />
	{:else if tab.type === 'session-context'}
		<ContextView {tab} {active} />
	{:else if isWikiOverview}
		<WikiView {tab} {active} />
	{:else if isWikiDetail}
		<WikiDetailView {tab} {active} />
	{:else if tab.type === 'wiki-list'}
		<WikiListView {tab} {active} />
	{:else if tab.type === 'data-sources'}
		<DataSourcesView {tab} {active} />
	{:else if tab.type === 'usage'}
		<UsageView {tab} {active} />
	{:else if tab.type === 'storage'}
		<StorageView {tab} {active} />
	{:else if tab.type === 'data-jobs'}
		<JobsView {tab} {active} />
	{:else if tab.type === 'profile'}
		<ProfileView {tab} {active} />
	{:else if tab.type === 'data-entities'}
		<EntitiesView {tab} {active} />
	{:else if tab.type === 'data-drive'}
		<DriveView {tab} {active} />
	{:else}
		<!-- Placeholder for views not yet extracted -->
		<div class="placeholder">
			<iconify-icon icon={placeholderInfo.icon} />
			<span class="title">{placeholderInfo.title}</span>
			{#if placeholderInfo.subtitle}
				<span class="subtitle">{placeholderInfo.subtitle}</span>
			{/if}
			<span class="coming-soon">View coming soon</span>
		</div>
	{/if}
</div>

<style>
	.tab-content {
		position: absolute;
		inset: 0;
		flex-direction: column;
		overflow: hidden;
	}

	.placeholder {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		height: 100%;
		color: var(--color-foreground-muted);
	}

	.placeholder iconify-icon {
		font-size: 48px;
		opacity: 0.4;
		margin-bottom: 8px;
	}

	.placeholder .title {
		font-size: 18px;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.placeholder .subtitle {
		font-size: 14px;
		opacity: 0.7;
	}

	.placeholder .coming-soon {
		font-size: 12px;
		margin-top: 16px;
		padding: 4px 12px;
		border-radius: 12px;
		background: var(--color-surface-elevated);
		opacity: 0.6;
	}
</style>
