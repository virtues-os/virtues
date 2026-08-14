<script lang="ts">
	import "../../app.css";
	// The shared grammar for all six onboarding screens — see the file header for
	// why it is global rather than per-component.
	import "$lib/components/onboarding/onboarding.css";
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
	offset="max(16px, env(safe-area-inset-top))"
	mobileOffset="max(16px, env(safe-area-inset-top))"
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
