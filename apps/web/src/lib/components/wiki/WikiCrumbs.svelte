<!--
	A wiki page's path: Wiki / People / Nick, or Wiki / 2026 / October /
	Tuesday, October 27 for a day.

	The parents are the indexes the Wiki panel lists the page under, so the
	path reads the same as the sidebar. The last part is the page itself and
	is not a link.

	On the desktop the path is the tab's action toolbar's (usePageCrumbs) and
	this draws nothing. On the phone it draws the breadcrumb: the day page
	puts it in its own bar, on the line with Article | Data, and pages
	without a bar pass `bar` to get a row of the same height.
-->
<script lang="ts">
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { usePageCrumbs } from "$lib/stores/paneActions.svelte";
	import { wikiCrumb, type Crumb } from "$lib/tabs/crumbs";

	interface Props {
		/** Below Wiki: the parents with their hrefs, then the page. */
		path: Crumb[];
		bar?: boolean;
	}

	let { path, bar = false }: Props = $props();

	const toolbar = usePageCrumbs(() => ({
		glyph: "wiki",
		crumbs: [wikiCrumb(), ...path],
	}));

	// A plain link changes the browser's URL and leaves the tab where it was,
	// so a click moves the tab itself, the way the day page steps between
	// days. The href stays for a modified click and for the link's semantics.
	function go(e: MouseEvent, href: string) {
		if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
		e.preventDefault();
		windowShellStore.navigate(href);
	}
</script>

{#if !toolbar.inToolbar}
	{#if bar}
		<div class="crumb-bar">{@render crumbs()}</div>
	{:else}
		{@render crumbs()}
	{/if}
{:else if bar}
	<!-- The page's first line keeps some air under the toolbar. -->
	<div class="crumb-space" aria-hidden="true"></div>
{/if}

{#snippet crumbs()}
	<nav class="crumbs" aria-label="Breadcrumb">
		<a href="/wiki" onclick={(e) => go(e, "/wiki")}>Wiki</a>
		{#each path as crumb, i (i)}
			<span class="sep" aria-hidden="true">/</span>
			{#if crumb.route && i < path.length - 1}
				<a href={crumb.route} onclick={(e) => go(e, crumb.route!)}>{crumb.label}</a>
			{:else if i < path.length - 1}
				<span>{crumb.label}</span>
			{:else}
				<span class="current" aria-current="page">{crumb.label}</span>
			{/if}
		{/each}
	</nav>
{/snippet}

<style>
	/* The day page's toolbar is 3.25rem tall; this row matches it. */
	.crumb-bar {
		display: flex;
		align-items: center;
		height: 3.25rem;
	}

	.crumb-space {
		height: 2rem;
	}

	.crumbs {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		min-width: 0;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	a {
		color: inherit;
		text-decoration: none;
	}

	a:hover {
		color: var(--color-foreground);
	}

	.current {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		color: var(--color-foreground-muted);
	}
</style>
