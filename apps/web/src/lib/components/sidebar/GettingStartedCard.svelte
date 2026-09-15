<!--
	The progress card: getting started's one mention outside its room. A
	colored card at the bottom of the sidebar, above Sources: the title and
	how many things are left, and it opens the room. From step 1 to
	graduation. It is the standing reminder that setup is unfinished — the
	app is not closed off while it is, so this is the only thing saying so.
-->
<script lang="ts">
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { GETTING_STARTED_CHAT_ID } from "$lib/components/chat/getting-started/getting-started";

	const open = $derived(gettingStarted.openCount);

	function go() {
		windowShellStore.openTabFromRoute(`/chat/${GETTING_STARTED_CHAT_ID}`, {
			label: "Getting started",
			focusExisting: true,
		});
	}
</script>

<button type="button" class="card" onclick={go}>
	<span class="title">Getting started</span>
	<span class="count">{open === 1 ? "1 thing left" : `${open} things left`}</span>
</button>

<style>
	.card {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.5rem;
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
	.title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.9375rem;
	}
	.count {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}
</style>
