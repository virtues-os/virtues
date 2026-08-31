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
	 * This is the uppercase label, chosen because it is a LABEL and not a
	 * title: settings headings name a category you are scanning past, not a
	 * heading you are reading. It stays quiet, and it leaves the right margin
	 * free for `note`.
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
		<h2 class="text-xs font-medium uppercase tracking-wide text-foreground-subtle">
			{title}
		</h2>
		{#if note}
			<span class="text-[11px] text-foreground-subtle font-normal normal-case tracking-normal">
				{note}
			</span>
		{/if}
	</div>
	{@render children()}
</div>
