<script lang="ts">
	import "../../app.css";
	import { onMount } from "svelte";
	import { Toaster } from "svelte-sonner";
	import { initTheme } from "$lib/utils/theme";

	let { children } = $props();

	onMount(() => {
		initTheme();
	});
</script>

<Toaster
	position="top-center"
	toastOptions={{
		style: `
			background: var(--surface);
			color: var(--foreground);
			border: 1px solid var(--border);
			font-family: var(--font-sans);
		`,
	}}
/>

<div class="min-h-screen overscroll-none bg-surface text-foreground">
	{@render children?.()}
</div>
