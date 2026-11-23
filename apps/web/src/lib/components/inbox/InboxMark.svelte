<script lang="ts">
	/**
	 * The mark a thing wears in every view of the Inbox, so one glance says
	 * what kind it is: a filled dot waits on you, ∴ is something your server
	 * noticed, a dashed ring is a task of yours, and a dotted ring is a
	 * request the assistant has not finished.
	 *
	 * In a row the mark is drawn as a sidebar icon is: an 18px box
	 * (`ROW_GLYPH_PX`), a ring as wide as a round Myna glyph in it (16px),
	 * and the sidebar's 1.25px line. The dot is half the ring, which gives it
	 * about the ink of a ring or the ∴. The radar draws its marks a little
	 * larger, on their own.
	 *
	 * A thing that names what it is about (`glyph`, a Myna icon) wears that
	 * icon instead, at the sidebar's size and line.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { ROW_GLYPH_PX } from '$lib/components/sidebar/AtlasIcon.svelte';
	import type { ItemKind } from '$lib/inbox/items';

	let {
		kind,
		glyph,
		radar = false,
	}: {
		kind: ItemKind;
		/** A Myna icon to wear in place of the kind's mark. */
		glyph?: string;
		/** Drawn on the radar, where a mark stands alone and can be larger. */
		radar?: boolean;
	} = $props();

	const box = $derived(radar ? 20 : ROW_GLYPH_PX);
	const ring = $derived(radar ? 15 : ROW_GLYPH_PX - 2);
	const dot = $derived(radar ? 10 : (ROW_GLYPH_PX - 2) / 2);
	/** The ∴ at the size the sidebar's header draws it (16px), centered in the box. */
	const logo = $derived(radar ? 20 : 16);
</script>

<span class="mark {kind}" style:--box="{box}px" style:--ring="{ring}px" style:--dot="{dot}px" style:--line={(1.25 * 24) / box} aria-hidden="true">
	{#if glyph}<Icon icon={glyph} width={box} height={box} />{:else if kind === 'insight'}<Icon icon="virtues:logo" width={logo} height={logo} />{:else}<span class="shape"></span>{/if}
</span>

<style>
	.mark {
		display: inline-grid;
		flex: none;
		place-items: center;
		width: var(--box);
		height: var(--box);
		vertical-align: middle;
		color: var(--color-primary);
	}
	/* A Myna icon's line, set to the sidebar's 1.25px of ink. */
	.mark :global(svg [stroke-width]) {
		stroke-width: var(--line);
	}
	.shape {
		box-sizing: border-box;
		width: var(--ring);
		height: var(--ring);
		border-radius: 50%;
	}
	.needs .shape {
		width: var(--dot);
		height: var(--dot);
		background: var(--color-primary);
	}
	.task .shape {
		border: 1.25px dashed var(--color-foreground);
		background: var(--color-page);
	}
	.waiting .shape {
		border: 1.25px dotted var(--color-primary);
		background: var(--color-page);
	}
</style>
