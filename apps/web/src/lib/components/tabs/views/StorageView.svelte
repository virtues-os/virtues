<script lang="ts">
	// Storage: what your server keeps that you file or save.
	//
	//   /storage           the front page: find, recently added, space
	//   /storage/drive/…   Files, the ones you filed     read-write
	//   /storage/media     App Media, what the app made  read-only
	//   /storage/trash     Recently deleted              restore or purge
	//
	// Bookmarks and Passwords are their own routes (/bookmarks, /passwords)
	// and are reached from the front page and the crumbs (STORAGE_MODE).
	// Streams, the raw records your sources send, is a section of Sources
	// (`/sources/streams`); its old route here forwards there.
	//
	// Files sits under /storage/drive, not /drive: its sub-paths are folder
	// names, so a tab at /drive/streams would be ambiguous with a folder
	// someone actually named "streams".
	import type { Tab } from "$lib/tabs/types";
	import DriveView from "./DriveView.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import AppMediaView from "./AppMediaView.svelte";
	import TrashView from "./TrashView.svelte";
	import StorageFront from "$lib/components/storage/StorageFront.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type Section = "front" | "drive" | "media" | "trash";

	// Streams moved to Sources; an old link or bookmark lands there.
	$effect(() => {
		if (tab.route === "/storage/streams") windowShellStore.navigate("/sources/streams", { label: "Sources · Streams" });
	});

	// The legacy `/trash` route lands on Recently deleted, and a folder path
	// (/storage/drive/Documents/2026) on Files.
	const section = $derived<Section>(
		tab.route === "/storage"
			? "front"
			: tab.route === "/trash"
				? "trash"
				: ((tab.route.match(/^\/storage\/(media|trash)$/)?.[1] as Section) ?? "drive"),
	);
</script>

<div class="flex h-full w-full flex-col">
	<div class="min-h-0 flex-1">
		{#if tab.route === "/storage/streams"}
			<!-- Forwarding to Sources · Streams (the effect above). -->
		{:else if section === "front"}
			<StorageFront />
		{:else if section === "drive"}
			<DriveView {tab} {active} />
		{:else if section === "media"}
			<AppMediaView />
		{:else if section === "trash"}
			<TrashView {tab} {active} />
		{/if}
	</div>
</div>
