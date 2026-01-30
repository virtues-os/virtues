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
		href: `/day/day_${todaySlug}`,
		icon: "ri:calendar-check-line",
	}}
	{collapsed}
/>

<SidebarNavItem
	item={{
		id: "wiki-people",
		type: "link",
		label: "People",
		href: "/person",
		icon: PAGE_TYPE_META.person.icon,
		pagespace: "person",
	}}
	{collapsed}
/>

<SidebarNavItem
	item={{
		id: "wiki-places",
		type: "link",
		label: "Places",
		href: "/place",
		icon: PAGE_TYPE_META.place.icon,
		pagespace: "place",
	}}
	{collapsed}
/>

<SidebarNavItem
	item={{
		id: "wiki-orgs",
		type: "link",
		label: "Orgs",
		href: "/org",
		icon: PAGE_TYPE_META.organization.icon,
		pagespace: "org",
	}}
	{collapsed}
/>

<SidebarNavItem
	item={{
		id: "wiki-drive",
		type: "link",
		label: "Drive",
		href: "/drive",
		icon: "ri:hard-drive-2-line",
		pagespace: "drive",
	}}
	{collapsed}
/>
