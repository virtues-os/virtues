<script lang="ts">
	import type { Snippet } from "svelte";

	/**
	 * One labeled band of a settings page. The heading is `.settings-label` in
	 * app.css, the one heading style every settings page shares.
	 *
	 * Titles use plain operator words (`Vitals`, `Storage`, `Processes`), since
	 * the reader is about to read the thing itself.
	 */
	let {
		title,
		first = false,
		children,
	}: {
		title: string;
		/** Drop the top margin; use on the first section under a page heading. */
		first?: boolean;
		children: Snippet;
	} = $props();
</script>

<div class={first ? "" : "mt-8"}>
	<h2 class="settings-label mb-2">{title}</h2>
	{@render children()}
</div>
