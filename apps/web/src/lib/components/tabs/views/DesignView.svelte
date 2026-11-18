<script lang="ts">
	/**
	 * /design: the design system. It renders agents/build/design.md, read
	 * from the repo at build time, with the live specimens the file places
	 * (lib/design/specimens.ts) and the current value beside every token it
	 * names. Editing the file is editing this page.
	 *
	 * The toolbar's sun and moon preview Light and Dark on the whole app
	 * without saving; leaving the page puts back the theme you chose.
	 */
	import { onDestroy } from 'svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import Page from '$lib/components/Page.svelte';
	import DesignDoc from '$lib/components/design/DesignDoc.svelte';
	import TableOfContents, { type TocHeading } from '$lib/components/TableOfContents.svelte';
	import { designDoc } from '$lib/design/doc';
	import { specimens, splitDoc } from '$lib/design/specimens';
	import { usePageActions } from '$lib/stores/paneActions.svelte';
	import { applyTheme, getTheme, getThemeChoice, type Theme } from '$lib/utils/theme';
	import type { Tab } from '$lib/tabs/types';

	let { active }: { tab: Tab; active: boolean } = $props();

	const parts = splitDoc(designDoc);
	const placed = new Set(parts.flatMap((p) => (p.kind === 'specimen' ? [p.name] : [])));
	const unplaced = [...specimens.keys()].filter((n) => !placed.has(n)).sort();

	const chosen = getThemeChoice();
	let preview = $state<Theme>(getTheme());

	function show(theme: Theme) {
		preview = theme;
		applyTheme(theme);
	}

	onDestroy(() => {
		if (getThemeChoice() !== chosen) applyTheme(chosen);
	});

	const toolbar = usePageActions(() => (active ? themeControls : null));

	let headings = $state<TocHeading[]>([]);
	// Page scrolls itself; the contents list follows its scroller.
	let host = $state<HTMLElement | null>(null);
	const scroller = $derived((host?.firstElementChild as HTMLElement | null) ?? null);
</script>

{#snippet themeControls()}
	<IconButton icon="mynaui:sun" label="Preview Light" pressed={preview === 'oxford'} onclick={() => show('oxford')} />
	<IconButton icon="mynaui:moon" label="Preview Dark" pressed={preview === 'asgard'} onclick={() => show('asgard')} />
{/snippet}

<div class="host" bind:this={host}>
	<Page maxWidth="detail">
		{#if !toolbar.inToolbar}
			<div class="controls">{@render themeControls()}</div>
		{/if}
		<div class="layout">
			<div class="doc">
				<DesignDoc {parts} onheadings={(h) => (headings = h)} />

				{#if unplaced.length > 0}
					<section class="unplaced">
						<h2>Specimens this file does not place</h2>
						<p>
							Each has a <code>.specimen.svelte</code> file, but no block in design.md shows it. Place it
							in the section it belongs to, or delete it.
						</p>
						<ul>
							{#each unplaced as name (name)}
								<li><code>{name}</code> <span>{specimens.get(name)?.path}</span></li>
							{/each}
						</ul>
					</section>
				{/if}
			</div>
			<aside class="toc">
				<TableOfContents {headings} scrollContainer={scroller} />
			</aside>
		</div>
	</Page>
</div>

<style>
	.host {
		height: 100%;
		container-type: inline-size;
	}

	.controls {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
	}

	.layout {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 48px;
		gap: 24px;
	}

	.toc {
		position: relative;
	}

	.unplaced {
		margin-top: 64px;
		font-family: var(--font-sans);
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
	}

	.unplaced h2 {
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-heading);
		color: var(--color-foreground);
		margin: 0 0 8px;
	}

	.unplaced ul {
		margin: 12px 0 0;
		padding: 0;
		list-style: none;
	}

	.unplaced li {
		display: flex;
		gap: 12px;
		padding: 8px 0;
	}

	@container (max-width: 900px) {
		.layout {
			grid-template-columns: minmax(0, 1fr);
		}
		.toc {
			display: none;
		}
	}
</style>
