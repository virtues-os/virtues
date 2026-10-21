<!--
	The breadcrumb at the top left of a wiki page: Wiki / People / Nick.

	The parent is the index the Wiki panel lists the page under, so the crumb
	reads the same path as the sidebar. The last part is the page itself and
	is not a link.

	The day page puts it in its own toolbar, on the line with Article | Data.
	Pages without a toolbar pass `bar` to get a row of the same height, so the
	crumb sits on the same line on every wiki page.
-->
<script lang="ts">
	interface Props {
		parent: { label: string; href: string };
		current: string;
		bar?: boolean;
	}

	let { parent, current, bar = false }: Props = $props();
</script>

{#if bar}
	<div class="crumb-bar">{@render crumbs()}</div>
{:else}
	{@render crumbs()}
{/if}

{#snippet crumbs()}
	<nav class="crumbs" aria-label="Breadcrumb">
		<a href="/wiki">Wiki</a>
		<span class="sep" aria-hidden="true">/</span>
		<a href={parent.href}>{parent.label}</a>
		<span class="sep" aria-hidden="true">/</span>
		<span class="current" aria-current="page">{current}</span>
	</nav>
{/snippet}

<style>
	/* The day page's toolbar is 3.25rem tall; this row matches it. */
	.crumb-bar {
		display: flex;
		align-items: center;
		height: 3.25rem;
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
