<!--
	WikiSidebarSection.svelte

	Minimal wiki navigation for the sidebar.
	Uses SidebarNavItem for consistent styling/animation.
-->

<script lang="ts">
	import { page } from "$app/state";
	import { PAGE_TYPE_META } from "$lib/wiki/types/base";
	import { getPageBySlug } from "$lib/wiki";
	import SidebarNavItem from "$lib/components/sidebar/SidebarNavItem.svelte";

	interface Props {
		collapsed?: boolean;
		baseAnimationDelay?: number;
	}

	let { collapsed = false, baseAnimationDelay = 0 }: Props = $props();

	// Today's date slug
	const todaySlug = new Date().toISOString().split("T")[0];

	// Stagger delay between items
	const STAGGER = 30;

	// Determine the current wiki page type from the URL
	const currentWikiPageType = $derived.by(() => {
		const pathname = page.url.pathname;

		// Check for list pages first
		if (pathname === "/wiki/people") return "people-list";
		if (pathname === "/wiki/places") return "places-list";
		if (pathname === "/wiki/orgs") return "orgs-list";
		if (pathname === "/wiki/things") return "things-list";

		// Check for individual wiki pages: /wiki/[slug]
		const match = pathname.match(/^\/wiki\/([^/]+)$/);
		if (match) {
			const slug = match[1];
			const wikiPage = getPageBySlug(slug);
			if (wikiPage) {
				return wikiPage.type;
			}
		}

		return null;
	});
</script>

<SidebarNavItem
	item={{
		id: "wiki-overview",
		type: "link",
		label: "Overview",
		href: "/wiki",
		icon: "ri:dashboard-line",
	}}
	{collapsed}
	animationDelay={baseAnimationDelay}
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
	animationDelay={baseAnimationDelay + STAGGER}
/>

<SidebarNavItem
	item={{
		id: "wiki-people",
		type: "link",
		label: "People",
		href: "/wiki/people",
		icon: PAGE_TYPE_META.person.icon,
		forceActive: currentWikiPageType === "person" || currentWikiPageType === "people-list",
	}}
	{collapsed}
	animationDelay={baseAnimationDelay + STAGGER * 2}
/>

<SidebarNavItem
	item={{
		id: "wiki-places",
		type: "link",
		label: "Places",
		href: "/wiki/places",
		icon: PAGE_TYPE_META.place.icon,
		forceActive: currentWikiPageType === "place" || currentWikiPageType === "places-list",
	}}
	{collapsed}
	animationDelay={baseAnimationDelay + STAGGER * 3}
/>

<SidebarNavItem
	item={{
		id: "wiki-orgs",
		type: "link",
		label: "Orgs",
		href: "/wiki/orgs",
		icon: PAGE_TYPE_META.organization.icon,
		forceActive: currentWikiPageType === "organization" || currentWikiPageType === "orgs-list",
	}}
	{collapsed}
	animationDelay={baseAnimationDelay + STAGGER * 4}
/>

<SidebarNavItem
	item={{
		id: "wiki-things",
		type: "link",
		label: "Things",
		href: "/wiki/things",
		icon: PAGE_TYPE_META.thing.icon,
		forceActive: currentWikiPageType === "thing" || currentWikiPageType === "things-list",
	}}
	{collapsed}
	animationDelay={baseAnimationDelay + STAGGER * 5}
/>
