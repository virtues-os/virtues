<script lang="ts">
	import "../../app.css";
	import { onMount } from "svelte";
	import { initTheme } from "$lib/utils/theme";

	let { children } = $props();

	onMount(() => {
		initTheme();
	});
</script>

<div class="public-layout">
	{@render children()}
</div>

<style>
	.public-layout {
		min-height: 100vh;
		background: var(--color-background);
		color: var(--color-foreground);
	}
</style>
