<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import ActionsPanel from '$lib/components/actions/ActionsPanel.svelte';

	// One flat list — the Actions/Templates/History sub-tabs are gone
	// (applets overhaul). Old /actions/<sub> routes still resolve here.
	let { tab, active }: { tab: Tab; active: boolean } = $props();
	void tab;
	void active;
</script>

<div class="actions-view">
	<main class="content">
		<ActionsPanel />
	</main>
</div>

<style>
	.actions-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.content {
		flex: 1;
		overflow-y: auto;
		padding: 1.25rem 1.5rem 2rem;
		max-width: 72rem;
		width: 100%;
		margin: 0 auto;
	}
</style>
