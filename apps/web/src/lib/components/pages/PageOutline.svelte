<!--
	PageOutline.svelte

	Table of contents, "Library drawer" style: a slim bar showing the current
	section; click it and a panel drops down with the full h1–h3 outline (active
	entry emphasized, the rest dimmed), over a blurred navy scrim. Click an entry
	to scroll the editor there. Scroll-spy keeps the bar's label and the active
	entry in sync as you read.

	Knows no editor: the editor hands over an `OutlineNav` (`treeOutlineNav`)
	that scrolls to a heading and says where one starts in its scroller.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { sameHeading, type OutlineNav, type PageHeading } from "$lib/components/pages/outline";

	let { headings, nav }: { headings: PageHeading[]; nav: OutlineNav | null } = $props();

	let open = $state(false);
	let active = $state<PageHeading | null>(null);

	const activeHeading = $derived(
		(active && headings.find((h) => sameHeading(h, active!))) ?? headings[0] ?? null,
	);

	function scrollTo(h: PageHeading) {
		open = false;
		if (!nav) return;
		nav.scrollTo(h);
		active = h;
	}

	// The active section = the last heading whose top has scrolled to/above the
	// top of the viewport (with a small threshold so it flips a touch early).
	function computeActive() {
		if (!nav || headings.length === 0) return;
		const threshold = nav.scroller.scrollTop + 96;
		let current: PageHeading = headings[0];
		for (const h of headings) {
			const top = nav.topOf(h);
			if (top === null) continue;
			if (top <= threshold) current = h;
			else break;
		}
		active = current;
	}

	// Scroll-spy off the editor's scroller.
	$effect(() => {
		if (!nav) return;
		const scroller = nav.scroller;
		computeActive();
		const onScroll = () => computeActive();
		scroller.addEventListener("scroll", onScroll, { passive: true });
		return () => scroller.removeEventListener("scroll", onScroll);
	});

	// Recompute when the outline itself changes (edits add/remove headings).
	$effect(() => {
		void headings;
		computeActive();
	});
</script>

{#if headings.length > 0}
	<div class="page-outline">
		<button class="outline-bar" onclick={() => (open = !open)} aria-expanded={open}>
			<span class="outline-current">{activeHeading?.text ?? "Contents"}</span>
			<Icon icon={open ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line"} width="18" />
		</button>

		{#if open}
			<button class="outline-scrim" aria-label="Close outline" onclick={() => (open = false)}></button>
			<nav class="outline-drawer" aria-label="Table of contents">
				{#each headings as h}
					<button
						class="outline-item level-{h.level}"
						class:active={activeHeading !== null && sameHeading(h, activeHeading)}
						onclick={() => scrollTo(h)}
					>
						{h.text}
					</button>
				{/each}
			</nav>
		{/if}
	</div>
{/if}

<style>
	/* No box of its own: the trigger is a cell of the top bar's grid, and
	   the drawer anchors to the bar (.page-topbar is the positioned ancestor). */
	.page-outline {
		display: contents;
	}

	/* The middle column of the top bar's grid (.page-topbar): centred, and
	   as narrow as the row needs, down to its chevron. */
	.outline-bar {
		all: unset;
		box-sizing: border-box;
		grid-column: 2;
		justify-self: center;
		z-index: var(--z-sticky);
		display: inline-flex;
		align-items: center;
		gap: 3px;
		min-width: 0;
		max-width: min(42ch, 100%);
		padding: 4px 8px 4px 12px;
		border-radius: var(--card-radius);
		cursor: pointer;
		font-family: var(--font-sans, ui-sans-serif, system-ui, sans-serif);
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
		transition:
			color 0.12s ease,
			background 0.12s ease;
	}
	.outline-bar:hover {
		color: var(--color-foreground);
		background: var(--hover-bg);
	}
	/* The chevron is what opens the outline when the title has no room. */
	.outline-bar > :global(:not(.outline-current)) {
		flex-shrink: 0;
	}
	.outline-current {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Blurred navy scrim over just the PAGE content (below the bar) — not the app
	   sidebar/chrome. Anchored to .page-topbar, which spans only the page column,
	   and starting at the bar's bottom so the bar itself stays clear. */
	.outline-scrim {
		all: unset;
		position: absolute;
		top: 100%;
		left: 0;
		right: 0;
		height: 100vh;
		z-index: var(--z-sticky);
		cursor: default;
		background: rgba(2, 6, 23, 0.32);
		backdrop-filter: blur(2px);
		-webkit-backdrop-filter: blur(2px);
		animation: outline-fade 0.14s ease-out;
	}

	.outline-drawer {
		position: absolute;
		top: 100%;
		left: 0;
		right: 0;
		z-index: var(--z-sticky);
		display: flex;
		flex-direction: column;
		padding: 0.4rem 1.25rem 0.7rem;
		background: var(--color-surface);
		border-bottom: 1px solid var(--color-border);
		box-shadow: var(--shadow-float);
		max-height: 60vh;
		overflow-y: auto;
		animation: outline-drop 0.16s cubic-bezier(0.22, 1, 0.36, 1);
	}

	.outline-item {
		all: unset;
		box-sizing: border-box;
		cursor: pointer;
		padding: 4px 8px;
		border-radius: var(--card-radius);
		font-family: var(--font-sans, ui-sans-serif, system-ui, sans-serif);
		font-size: 0.875rem;
		line-height: 1.35;
		color: var(--color-foreground-subtle);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		transition:
			color 0.12s ease,
			background 0.12s ease;
	}
	.outline-item:hover {
		background: var(--hover-bg);
		color: var(--color-foreground-muted);
	}
	/* The heading you are in, by ink alone: weight never signals state (design.md). */
	.outline-item.active {
		color: var(--color-foreground);
	}

	.level-1 { padding-left: 0.5rem; }
	.level-2 { padding-left: 1.5rem; }
	.level-3 { padding-left: 2.5rem; }

	@keyframes outline-drop {
		from { opacity: 0; transform: translateY(-6px); }
		to { opacity: 1; transform: translateY(0); }
	}
	@keyframes outline-fade {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	@media (prefers-reduced-motion: reduce) {
		.outline-scrim,
		.outline-drawer {
			animation: none;
		}
	}
</style>
