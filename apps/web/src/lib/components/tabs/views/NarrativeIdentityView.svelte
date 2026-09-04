<!--
	NarrativeIdentityView.svelte — a redirect stub.

	The old three-register editor ("Past / Present / Future") edited the
	abridged narrative-identity copy, which was retired 2026-09-01 — the
	document is the one artifact now, and it lives at /wiki/identity (read)
	and on its own page (edit). This stub survives only so that old serialized
	tabs and deep links to /narrative-identity land somewhere sensible.
-->

<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { onMount } from "svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";

	let { tab }: { tab: Tab; active?: boolean } = $props();

	onMount(() => {
		windowShellStore.updateTab(tab.id, { route: "/wiki/identity" });
	});
</script>

<div class="redirect">Moved to the wiki — opening…</div>

<style>
	.redirect {
		padding: 2rem;
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}
</style>
