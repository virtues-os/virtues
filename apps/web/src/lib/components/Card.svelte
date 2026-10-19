<script lang="ts">
	import type { Snippet } from "svelte";

	/**
	 * The bordered surface every settings view was drawing by hand.
	 *
	 * A sweep on 2026-08-31 found 22 hand-rolled `rounded-lg border
	 * border-border` blocks across nine files, disagreeing about padding
	 * (`p-4` and `p-5`) and about whether they carry `bg-surface` at all.
	 *
	 * None of those disagreements were decisions. They are what happens when a
	 * shape is copied rather than named, and they are the reason the settings
	 * surface reads as assembled rather than designed. One card, chosen once.
	 *
	 * NOT every bordered box belongs here. The QR backdrop in DevicesView is
	 * `bg-white` on purpose — a scanner needs a light quiet zone whatever the
	 * theme — and wrapping it in a themed surface would break it. A card is
	 * chrome; that one is part of the image.
	 *
	 * Two arrangements, because the wild only ever contained two:
	 *
	 *   <Card>            one padded surface
	 *   <Card list>       rows divided by hairlines, each row padding itself
	 *
	 * `list` deliberately drops the padding rather than making it configurable:
	 * a divided list whose container is also padded puts a gutter outside the
	 * first divider, which is exactly the misalignment the copies produced.
	 *
	 * Written in Tailwind until 2026-09-16, when the radius moved from
	 * `rounded-lg` (8px) to the 12px design-grammar.md §6 specifies for a card,
	 * and the rules moved into scoped CSS so that a card reads the same way the
	 * 192 components around it are written. A card is a hairline on a surface
	 * and never a shadow: it reads as separate because it can be acted on, not
	 * because it floats.
	 */
	let {
		list = false,
		padding = "md",
		class: className = "",
		children,
	}: {
		/** Rows separated by hairlines; each row supplies its own padding. */
		list?: boolean;
		/** Ignored when `list` — see above. */
		padding?: "none" | "sm" | "md";
		class?: string;
		children: Snippet;
	} = $props();

	const pad = $derived(list ? "none" : padding);
</script>

<div class="v-card {className}" data-pad={pad} class:is-list={list}>
	{@render children()}
</div>

<style>
	.v-card {
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
	}

	.v-card[data-pad="sm"] {
		padding: 12px;
	}

	.v-card[data-pad="md"] {
		padding: 16px;
	}

	/* A hairline between rows, and none above the first or below the last.
	   Both shapes, because the wild contains both: rows as direct children,
	   and rows inside one <ul> (how DeviceView — the only real call site —
	   writes it). Matching only direct children is why the divided list this
	   replaced drew no hairlines at all there: the single <ul> has no sibling,
	   so `* + *` never matched and the bug was invisible in review. */
	.is-list > :global(* + *),
	.is-list > :global(ul) > :global(li + li),
	.is-list > :global(ol) > :global(li + li) {
		border-top: 1px solid var(--color-border);
	}

	.is-list > :global(ul),
	.is-list > :global(ol) {
		margin: 0;
		padding: 0;
		list-style: none;
	}
</style>
