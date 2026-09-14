<!--
	The progress card: getting started's one mention outside its room. A
	colored card at the bottom of the sidebar, above Sources — the four
	steps as marks and the open count — that opens the room. From step 1 to
	graduation; while the app is locked it is the only thing on the shelf.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { GETTING_STARTED_CHAT_ID } from "$lib/components/chat/getting-started/getting-started";

	const steps = $derived(gettingStarted.steps);
	const open = $derived(gettingStarted.openCount);
	const locked = $derived(gettingStarted.locked);

	function go() {
		windowShellStore.openTabFromRoute(`/chat/${GETTING_STARTED_CHAT_ID}`, {
			label: "Getting started",
			focusExisting: true,
		});
	}
</script>

<button type="button" class="card" class:locked onclick={go}>
	<div class="head">
		<span class="title">Getting started</span>
		<span class="count">{open === 0 ? "done" : open === 1 ? "1 thing left" : `${open} things left`}</span>
	</div>
	<ol class="steps">
		{#each steps as s (s.id)}
			<li class="step" class:done={s.status === "done"} class:skipped={s.status === "skipped"}>
				<span class="mark" aria-hidden="true">
					{#if s.status === "done"}<Icon icon="ri:check-line" width="11" />{:else if s.status === "skipped"}<Icon icon="ri:subtract-line" width="11" />{/if}
				</span>
				<span class="name">{s.title}</span>
			</li>
		{/each}
	</ol>
	{#if locked}
		<p class="note">Connect AI to open the rest.</p>
	{/if}
</button>

<style>
	.card {
		display: block;
		width: calc(100% - 1.25rem);
		margin: 0.5rem 0.625rem;
		padding: 0.625rem 0.75rem;
		text-align: left;
		font: inherit;
		cursor: pointer;
		border-radius: 8px;
		border: 1px solid color-mix(in srgb, var(--color-primary) 35%, transparent);
		background: color-mix(in srgb, var(--color-primary) 9%, transparent);
		color: var(--color-foreground);
	}
	.card:hover {
		background: color-mix(in srgb, var(--color-primary) 14%, transparent);
	}
	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.5rem;
		margin-bottom: 0.4rem;
	}
	.title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.9375rem;
	}
	.count {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}
	.steps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.15rem;
	}
	.step {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		font-size: 0.8125rem;
	}
	.mark {
		width: 0.9rem;
		height: 0.9rem;
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
	.note {
		margin: 0.5rem 0 0;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}
</style>
