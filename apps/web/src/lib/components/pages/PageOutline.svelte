<!--
	PageOutline.svelte

	Table of contents: a slim bar in the top bar's middle naming the section
	you are in; click it and the h1–h3 outline opens under it as a menu, the
	page left as it is behind it (no scrim). Click an entry to scroll the
	editor there. Scroll-spy keeps the bar's label and the current entry in
	sync as you read.

	Knows no editor: the editor hands over an `OutlineNav` (`treeOutlineNav`)
	that scrolls to a heading and says where one starts in its scroller.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { Popover } from "$lib/floating";
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
		<Popover bind:open placement="bottom" offset={4}>
			{#snippet trigger({ toggle })}
				<button class="outline-bar" onclick={toggle} aria-expanded={open} aria-haspopup="menu">
					<span class="outline-current">{activeHeading?.text ?? "Contents"}</span>
					<Icon icon={open ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line"} width="16" />
				</button>
			{/snippet}
			{#snippet children()}
				<nav class="outline-menu" aria-label="Table of contents">
					{#each headings as h}
						<button
							class="outline-item"
							style:--level={h.level}
							class:active={activeHeading !== null && sameHeading(h, activeHeading)}
							aria-current={activeHeading !== null && sameHeading(h, activeHeading) ? "location" : undefined}
							onclick={() => scrollTo(h)}
						>
							{h.text}
						</button>
					{/each}
				</nav>
			{/snippet}
		</Popover>
	</div>
{/if}

<style>
	/* No box of its own: the trigger's wrapper is the middle cell of the top
	   bar's grid (.page-topbar), centred, and as narrow as the row needs. */
	.page-outline {
		display: contents;
	}
	.page-outline :global(.popover-trigger) {
		grid-row: 1;
		grid-column: 2;
		justify-self: center;
		min-width: 0;
		max-width: min(42ch, 100%);
	}

	.outline-bar {
		all: unset;
		box-sizing: border-box;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		min-width: 0;
		max-width: 100%;
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
	.outline-bar:hover,
	.outline-bar[aria-expanded="true"] {
		color: var(--color-foreground);
		background: var(--hover-bg);
	}
	.outline-bar:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
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

	/* A menu, as the toolbar's are: MenuItem's rows, each heading indented
	   16px a level. */
	.outline-menu {
		display: flex;
		flex-direction: column;
		min-width: 240px;
		max-width: 360px;
		max-height: 60vh;
		overflow-y: auto;
		padding: 4px;
	}

	.outline-item {
		all: unset;
		box-sizing: border-box;
		cursor: pointer;
		padding: 6px 10px 6px calc(10px + (var(--level) - 1) * 16px);
		border-radius: var(--card-radius);
		font-family: var(--font-sans, ui-sans-serif, system-ui, sans-serif);
		font-size: 13px;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		transition: background-color 0.12s ease;
	}
	.outline-item:hover,
	.outline-item:focus-visible {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}
	/* The heading you are in, by ink alone: weight never signals state (design.md). */
	.outline-item.active {
		color: var(--color-foreground);
	}
</style>
