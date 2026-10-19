<script lang="ts">
	import StateBlock from "$lib/components/StateBlock.svelte";
	import type { Snippet } from "svelte";

	/** A pane with nothing in it yet. The shell is [StateBlock](StateBlock.svelte). */
	let {
		icon = "ri:inbox-line",
		title,
		message,
		class: className = "",
		actions,
	}: {
		icon?: string;
		title?: string;
		message?: string;
		class?: string;
		actions?: Snippet;
	} = $props();
</script>

<StateBlock {icon} iconSize={40} {title} {message} class={className} {actions} />
