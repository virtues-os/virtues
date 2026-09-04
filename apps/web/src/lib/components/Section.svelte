<script lang="ts">
	import type { Snippet } from "svelte";

	/**
	 * One labelled band of a settings page.
	 *
	 * Settings had two heading systems: sixteen files used a bare `<h2>`, and
	 * the device pages used a small uppercase label. Which one you met depended
	 * on which page you had landed on, so neighbouring sections of the same
	 * product disagreed about what a section even looks like.
	 *
	 * The heading treatment is `.settings-label` in app.css — one definition,
	 * shared with the views that still write their own `<h2 class=…>`. It was
	 * an uppercase letterspaced micro-label until 2026-09-03; see that comment
	 * for why a serif with no small-cap cut should not be asked to fake one.
	 * It still leaves the right margin free for `note`.
	 *
	 * `note` is that margin — a short, subordinate fact about where the
	 * section's contents came from ("measured here", "as your server heard
	 * it"). It exists because a page that draws on two sources has to say
	 * which is which, or two sections disagreeing reads as a bug rather than
	 * as two vantages. See DeviceView, where it was introduced.
	 *
	 * VOICE: sysadmin, not poetic. `Vitals`, `Storage`, `Processes` — the words
	 * an operator would use for the thing they are about to read. Settings is
	 * an instrument panel; "The face" and "What your server made of it" belong to
	 * surfaces that are telling you a story, and this is not one.
	 */
	let {
		title,
		note,
		first = false,
		children,
	}: {
		title: string;
		/** Short right-margin aside — provenance, scope, or units. */
		note?: string;
		/** Drop the top margin; use on the first section under a page heading. */
		first?: boolean;
		children: Snippet;
	} = $props();
</script>

<div class={first ? "" : "mt-8"}>
	<div class="flex items-baseline justify-between gap-3 mb-2">
		<h2 class="settings-label mb-0">
			{title}
		</h2>
		{#if note}
			<span class="text-[11px] text-foreground-subtle font-normal">
				{note}
			</span>
		{/if}
	</div>
	{@render children()}
</div>
