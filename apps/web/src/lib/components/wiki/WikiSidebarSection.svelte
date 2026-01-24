<!--
	WikiSidebarSection.svelte

	Minimal wiki navigation for the sidebar.
	Uses SidebarNavItem for consistent styling/animation.
-->

<script lang="ts">
	import { PAGE_TYPE_META } from "$lib/wiki/types/base";
	import SidebarNavItem from "$lib/components/sidebar/SidebarNavItem.svelte";

	interface Props {
		collapsed?: boolean;
	}

	let { collapsed = false }: Props = $props();

	// Today's date slug
	const todaySlug = new Date().toISOString().split("T")[0];
</script>

<SidebarNavItem
	item={{
		id: "wiki-overview",
		type: "link",
		label: "Overview",
		href: "/wiki",
		icon: "ri:dashboard-line",
		pagespace: "wiki",
	}}
	{collapsed}
/>

<SidebarNavItem
	item={{
		id: "wiki-today",
		type: "link",
		label: "Today",
		href: `/wiki/${todaySlug}`,
		icon: "ri:calendar-check-line",
	}}
	{collapsed}
/>

<SidebarNavItem
	item={{
		id: "wiki-people",
		type: "link",
		label: "People",
		href: "/wiki/people",
		icon: PAGE_TYPE_META.person.icon,
		pagespace: "wiki/people",
	}}
	{collapsed}
/>

<SidebarNavItem
	item={{
		id: "wiki-places",
		type: "link",
		label: "Places",
		href: "/wiki/places",
		icon: PAGE_TYPE_META.place.icon,
		pagespace: "wiki/places",
	}}
	{collapsed}
/>

<SidebarNavItem
	item={{
		id: "wiki-orgs",
		type: "link",
		label: "Orgs",
		href: "/wiki/orgs",
		icon: PAGE_TYPE_META.organization.icon,
		pagespace: "wiki/orgs",
	}}
	{collapsed}
/>

<SidebarNavItem
	item={{
		id: "wiki-things",
		type: "link",
		label: "Things",
		href: "/wiki/things",
		icon: PAGE_TYPE_META.thing.icon,
		pagespace: "wiki/things",
	}}
	{collapsed}
/>

<SidebarNavItem
	item={{
		id: "wiki-drive",
		type: "link",
		label: "Drive",
		href: "/data/drive",
		icon: "ri:hard-drive-2-line",
		pagespace: "data/drive",
	}}
	{collapsed}
/>
