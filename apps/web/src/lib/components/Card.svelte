<script lang="ts">
	import { twMerge } from "tailwind-merge";
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

	const pad = $derived(list || padding === "none" ? "" : padding === "sm" ? "p-3" : "p-4");
</script>

<div
	class={twMerge(
		"rounded-lg border border-border bg-surface",
		list ? "divide-y divide-border" : pad,
		className,
	)}
>
	{@render children()}
</div>
