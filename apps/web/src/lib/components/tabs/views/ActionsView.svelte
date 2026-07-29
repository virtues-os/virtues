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
		<div class="measure">
			<ActionsPanel />
		</div>
	</main>
</div>

<style>
	.actions-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	/* Mirrors the Page shell's box model — padding on the outer scroller, the
	   `wide` (72rem) measure centred inside it — so this hand-rolled view lines
	   up with the rooms built from Page. It is hand-rolled only because
	   ActionsPanel owns its own header row. */
	.content {
		flex: 1;
		overflow-y: auto;
		padding: 3rem;
	}

	.measure {
		max-width: 72rem;
		width: 100%;
		margin: 0 auto;
	}
</style>
