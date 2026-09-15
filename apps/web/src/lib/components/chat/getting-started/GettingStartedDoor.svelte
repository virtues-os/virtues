<!--
	One door, out of the room and Home. It used to be the box's only exit
	while the app was closed off, and carried a warning to match; nothing is
	closed off now, so leaving is just leaving — the room keeps its place,
	the sidebar card says setup is unfinished, and you come back when you
	like. Deliberately quiet: something to find, not something offered.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import Icon from "$lib/components/Icon.svelte";

	async function leave() {
		await goto("/home");
	}
</script>

<button
	type="button"
	class="door"
	onclick={leave}
	title="Come back to this later"
	aria-label="Come back to this later"
>
	<Icon icon="ri:door-open-line" width="16" />
</button>

<style>
	.door {
		display: inline-flex;
		align-items: center;
		background: none;
		border: 1px solid transparent;
		border-radius: 6px;
		padding: 0.25rem 0.4rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
		flex: none;
	}
	.door:hover {
		color: var(--color-foreground);
		border-color: var(--color-border);
	}
</style>
