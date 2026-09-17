<script lang="ts">
	import StateBlock from "$lib/components/StateBlock.svelte";

	/** A pane waiting on its data. The shell is [StateBlock](StateBlock.svelte). */
	let {
		message,
		class: className = "",
	}: {
		message?: string;
		class?: string;
	} = $props();
</script>

<StateBlock icon="ri:loader-4-line" iconSize={24} spin {message} class={className} />
