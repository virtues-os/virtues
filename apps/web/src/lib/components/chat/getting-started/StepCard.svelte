<!--
	The shell every step card shares: a title, one line of what it is, the
	body when open, and the skip where a step can be skipped. The first open
	step opens by default; the rest wait folded until asked.
-->
<script lang="ts">
	import type { Snippet } from "svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import type { GettingStartedStepId } from "$lib/api/client";

	interface Props {
		step: GettingStartedStepId;
		title: string;
		what: string;
		skippable?: boolean;
		/** Open regardless of position (a `show_step` tool part). */
		forceOpen?: boolean;
		children: Snippet;
	}
	let { step, title, what, skippable = false, forceOpen = false, children }: Props = $props();

	let toggled = $state<boolean | null>(null);
	const open = $derived(toggled ?? (forceOpen || gettingStarted.firstOpen === step));
	const status = $derived(gettingStarted.step(step)?.status ?? "open");
</script>

<section class="card" class:open data-gs-step={step}>
	<div class="head">
		<button type="button" class="titlebtn" onclick={() => (toggled = !open)} aria-expanded={open}>
			<span class="title">{title}</span>
			{#if !open}<span class="what">{what}</span>{/if}
		</button>
		{#if skippable && status === "open"}
			<button type="button" class="skip" onclick={() => void gettingStarted.skip(step, true)}>Skip</button>
		{/if}
	</div>
	{#if open}
		<p class="what lead">{what}</p>
		<div class="body">
			{@render children()}
		</div>
	{/if}
</section>

<style>
	.card {
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 0.875rem 1rem;
		background: var(--color-background);
	}
	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
	}
	.titlebtn {
		background: none;
		border: 0;
		padding: 0;
		font: inherit;
		text-align: left;
		cursor: pointer;
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		color: var(--color-foreground);
	}
	.title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.125rem;
	}
	.what {
		color: var(--color-foreground-muted);
		font-size: 0.9rem;
	}
	.lead {
		margin: 0.5rem 0 0.75rem;
	}
	.skip {
		background: none;
		border: 0;
		padding: 0;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		flex: none;
	}
	.skip:hover {
		color: var(--color-foreground);
	}
	.body {
		margin-top: 0.25rem;
	}
</style>
