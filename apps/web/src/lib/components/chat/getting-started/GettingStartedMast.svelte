<!--
	The mast at the top of the getting-started room: the title, one standfirst
	in the letter's voice, the four steps with a mark against each, and the
	door at the right. No count, no progress bar. Skipped steps offer their
	way back here, because the room is where they were skipped.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import GettingStartedDoor from "./GettingStartedDoor.svelte";
	import type { GettingStartedStepId } from "$lib/api/client";

	interface Props {
		/** Scroll a step's card into view, if it has one. */
		onOpenStep?: (step: GettingStartedStepId) => void;
	}
	let { onOpenStep }: Props = $props();

	const steps = $derived(gettingStarted.steps);
	const graduated = $derived(gettingStarted.graduated);
</script>

<header class="mast" class:graduated>
	<div class="row">
		<h1 class="title">Getting started</h1>
		<GettingStartedDoor />
	</div>
	{#if graduated}
		<p class="standfirst">All four are settled. This room stays for questions about the setup; the rest of the app is yours.</p>
	{:else}
		<p class="standfirst">
			Your server keeps the record of your life. These four things are what it cannot do for itself.
		</p>
	{/if}
	<ol class="steps">
		{#each steps as s (s.id)}
			<li class="step" class:done={s.status === "done"} class:skipped={s.status === "skipped"}>
				<span class="mark" aria-hidden="true">
					{#if s.status === "done"}
						<Icon icon="ri:check-line" width="14" />
					{:else if s.status === "skipped"}
						<Icon icon="ri:subtract-line" width="14" />
					{:else if s.underway}
						<Icon icon="ri:more-line" width="14" />
					{/if}
				</span>
				{#if s.status === "open"}
					<button type="button" class="name link" onclick={() => onOpenStep?.(s.id)}>{s.title}</button>
				{:else}
					<span class="name">{s.title}</span>
				{/if}
				{#if s.status === "done" && s.via === "byo"}
					<span class="note">your own endpoint</span>
				{:else if s.status === "done" && s.via === "subscription"}
					<span class="note">Virtues subscription</span>
				{:else if s.underway}
					<span class="note">underway</span>
				{:else if s.status === "skipped"}
					<button type="button" class="note undo" onclick={() => void gettingStarted.skip(s.id, false)}>skipped · undo</button>
				{/if}
			</li>
		{/each}
	</ol>
</header>

<style>
	.mast {
		padding: 0.25rem 0 0.75rem;
		border-bottom: 1px solid var(--color-border);
		margin-bottom: 0.5rem;
	}
	.row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
	}
	.title {
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		font-size: 1.75rem;
		line-height: 1.15;
		margin: 0;
		color: var(--color-foreground);
	}
	.standfirst {
		margin: 0.5rem 0 0.875rem;
		color: var(--color-foreground-muted);
		max-width: 38rem;
	}
	.steps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.3rem;
	}
	.step {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}
	.mark {
		width: 1.1rem;
		height: 1.1rem;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		color: var(--color-foreground-subtle);
		flex: none;
	}
	.step.done .mark {
		border-color: var(--color-foreground);
		color: var(--color-foreground);
	}
	.step.done .name,
	.step.skipped .name {
		color: var(--color-foreground-muted);
	}
	.name {
		background: none;
		border: 0;
		padding: 0;
		font: inherit;
		color: inherit;
		text-align: left;
	}
	.name.link {
		cursor: pointer;
	}
	.name.link:hover {
		text-decoration: underline;
	}
	.note {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}
	.undo {
		background: none;
		border: 0;
		padding: 0;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}
	.undo:hover {
		color: var(--color-foreground);
	}
</style>
