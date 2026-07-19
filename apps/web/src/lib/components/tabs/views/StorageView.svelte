<script lang="ts">
	// Storage — the four kinds of bytes the box holds, as tabs.
	//
	//   Drive       files you filed          read-write
	//   Streams     raw evidence + its blobs read-only
	//   App Media   assets the app made/uses read-only
	//   Trash       deleted Drive files      restore/purge
	//
	// These were previously scattered (/drive, /trash, /developers/lake) and one of
	// them — the lake — was a stub rendering zeros. They are one surface because
	// they answer one question: what is on this disk, and who put it there.
	//
	// Note the base is /storage, NOT /drive: Drive's sub-paths are user folder
	// names (/storage/drive/Documents/…), so a tab at /drive/streams would be
	// ambiguous with a folder someone actually named "streams".
	import type { Tab } from "$lib/tabs/types";
	import SubNav, { type SubNavItem } from "$lib/components/SubNav.svelte";
	import DriveView from "./DriveView.svelte";
	import StreamsView from "./StreamsView.svelte";
	import AppMediaView from "./AppMediaView.svelte";
	import TrashView from "./TrashView.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type SubTab = "drive" | "streams" | "media" | "trash";

	// Derived from the route; SubNav owns the writing side. A drive folder path
	// (/storage/drive/Documents/2026) still resolves to the `drive` tab.
	//
	// The legacy `/trash` route must land on the Trash tab, not silently fall
	// through to Drive — old links and the sidebar still point at it.
	const subTab = $derived<SubTab>(
		tab.route === "/trash"
			? "trash"
			: ((tab.route.match(/^\/storage\/(streams|media|trash)$/)?.[1] as SubTab) ?? "drive"),
	);

	const tabs: SubNavItem[] = [
		{ id: "drive", label: "Drive" },
		{ id: "streams", label: "Streams" },
		{ id: "media", label: "App Media" },
		{ id: "trash", label: "Trash" },
	];
</script>

<div class="flex h-full w-full flex-col bg-background">
	<SubNav
		tabId={tab.id}
		route={tab.route}
		base="/storage"
		default="drive"
		items={tabs}
		divider
		ariaLabel="Storage sections"
	/>

	<div class="min-h-0 flex-1">
		{#if subTab === "drive"}
			<DriveView {tab} {active} />
		{:else if subTab === "streams"}
			<StreamsView />
		{:else if subTab === "media"}
			<AppMediaView />
		{:else if subTab === "trash"}
			<TrashView {tab} {active} />
		{/if}
	</div>
</div>
