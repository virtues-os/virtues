<script lang="ts">
	import type { Snippet } from "svelte";

	/**
	 * One labelled reading inside a `<Card list>`.
	 *
	 * Exists for the distinction in `unknown`, which every screen in this app
	 * has got wrong at least once: **"we could not ask" is not "the answer is
	 * nothing."**
	 *
	 * That conflation is the through-line of a whole week of bugs. A box that
	 * could not be reached rendered as setup-complete, and put a new owner in
	 * an empty chat. A collector that had never reported its build rendered as
	 * `1.0.0`, and sent someone hunting a stale binary that did not exist. An
	 * update check that failed rendered the same as one that passed, and a
	 * server sat ten days behind while every screen looked healthy. In each
	 * case the missing state was not `null` — it was *unread*, and it was drawn
	 * as though it had been read.
	 *
	 * So three renderings, not two:
	 *
	 *   value    "1.0.26"   we asked, this is the answer
	 *   none     —          we asked; there is genuinely nothing
	 *   unknown  —          we could not ask, and you should not trust a blank
	 *
	 * `unknown` gets a dotted rule under it and says so on hover. Deliberately
	 * quiet: this is not an error and must not shout, but it must not be
	 * mistakable for a measured zero either. A reader scanning a column of
	 * values should be able to see which ones were actually read.
	 *
	 * `note` is the line beneath the value — freshness, units, a verdict. It
	 * carries the tone, because the value itself is a fact and facts are not
	 * coloured; only what we make of them is.
	 */
	let {
		label,
		hint,
		value = null,
		unknown = false,
		note,
		tone = "muted",
		mono = false,
		action,
	}: {
		label: string;
		/** What this reading GOVERNS — the sentence that makes a number mean something. */
		hint?: string;
		/** The answer, or null when we asked and there is none. */
		value?: string | null;
		/** True when we could not ask at all. Beats `value`. */
		unknown?: boolean;
		/** Secondary line under the value: freshness, a verdict, a unit. */
		note?: string;
		tone?: "muted" | "warning" | "info" | "success";
		mono?: boolean;
		/** Trailing control — a button, usually. */
		action?: Snippet;
	} = $props();
</script>

<li class="p-4 flex items-center gap-3">
	<div class="flex-1 min-w-0">
		<div class="text-sm text-foreground">{label}</div>
		{#if hint}
			<div class="text-xs text-foreground-subtle mt-0.5">{hint}</div>
		{/if}
	</div>

	<div class="text-right shrink-0">
		{#if unknown}
			<span
				class="text-xs text-foreground-subtle border-b border-dotted border-foreground-subtle cursor-help"
				title="Not reported — this could not be read, so it is not a measurement"
			>
				—
			</span>
		{:else}
			<div class="text-xs text-foreground-muted" class:font-mono={mono}>{value ?? "—"}</div>
		{/if}
		{#if note}
			<div
				class="text-[11px] mt-0.5"
				class:text-warning={tone === "warning"}
				class:text-info={tone === "info"}
				class:text-success={tone === "success"}
				class:text-foreground-subtle={tone === "muted"}
			>
				{note}
			</div>
		{/if}
	</div>

	{#if action}{@render action()}{/if}
</li>
