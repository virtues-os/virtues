<script lang="ts">
	/**
	 * One insight's page: the insight whole (InsightDetail) in the page frame.
	 * The tab is named for what the insight is about, since the insight's
	 * sentence is too long for a tab.
	 */
	import type { Tab } from '$lib/tabs/types';
	import type { NoteDetail } from '$lib/api/client';
	import InsightDetail from '$lib/components/threads/InsightDetail.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	let { tab }: { tab: Tab; active?: boolean } = $props();

	const key = $derived.by(() => {
		const m = tab.route.match(/^\/insight\/([^?#]+)/);
		return m ? decodeURIComponent(m[1]) : null;
	});
	let detail = $state<NoteDetail | null>(null);

	$effect(() => {
		const label = detail ? `Insight about ${detail.about}` : null;
		if (label && tab.label !== label) windowShellStore.updateTab(tab.id, { label });
	});
</script>

<div class="insight-page">
	<InsightDetail {key} bind:current={detail} />
</div>

<style>
	.insight-page {
		width: 100%;
		height: 100%;
		overflow-y: auto;
	}
</style>
