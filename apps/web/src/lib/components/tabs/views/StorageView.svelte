<script lang="ts">
	// Storage — the kinds of bytes the box holds that you file or keep, as tabs.
	//
	//   Drive       files you filed          read-write
	//   App Media   assets the app made/uses read-only
	//   Recently deleted   everything deleted in the app   restore/purge
	//
	// Streams, the raw evidence your sources send, is a section of Sources
	// (`/sources/streams`); its old route here forwards there.
	//
	// Note the base is /storage, NOT /drive: Drive's sub-paths are user folder
	// names (/storage/drive/Documents/…), so a tab at /drive/streams would be
	// ambiguous with a folder someone actually named "streams".
	import type { Tab } from "$lib/tabs/types";
	import SubNav, { type SubNavItem } from "$lib/components/SubNav.svelte";
	import DriveView from "./DriveView.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import AppMediaView from "./AppMediaView.svelte";
	import TrashView from "./TrashView.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type SubTab = "drive" | "media" | "trash";

	// Streams moved to Sources; an old link or bookmark lands there.
	$effect(() => {
		if (tab.route === "/storage/streams") windowShellStore.navigate("/sources/streams", { label: "Sources · Streams" });
	});

	// Derived from the route; SubNav owns the writing side. A drive folder path
	// (/storage/drive/Documents/2026) still resolves to the `drive` tab.
	//
	// The legacy `/trash` route must land on the Trash tab, not silently fall
	// through to Drive — old links and the sidebar still point at it.
	const subTab = $derived<SubTab>(
		tab.route === "/trash"
			? "trash"
			: ((tab.route.match(/^\/storage\/(media|trash)$/)?.[1] as SubTab) ?? "drive"),
	);

	const tabs: SubNavItem[] = [
		{ id: "drive", label: "Drive" },
		{ id: "media", label: "App Media" },
		{ id: "trash", label: "Recently deleted" },
	];
</script>

<div class="flex h-full w-full flex-col bg-background">
	{#if tab.route !== "/storage/streams"}
		<SubNav
			tabId={tab.id}
			route={tab.route}
			base="/storage"
			default="drive"
			items={tabs}
			divider
			ariaLabel="Storage sections"
		/>
	{/if}

	<div class="min-h-0 flex-1">
		{#if tab.route === "/storage/streams"}
			<!-- Forwarding to Sources · Streams (the effect above). -->
		{:else if subTab === "drive"}
			<DriveView {tab} {active} />
		{:else if subTab === "media"}
			<AppMediaView />
		{:else if subTab === "trash"}
			<TrashView {tab} {active} />
		{/if}
	</div>
</div>
