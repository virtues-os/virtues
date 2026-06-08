<script lang="ts">
	import "../../app.css";
	import { onMount } from "svelte";
	import { Toaster } from "svelte-sonner";

	import { initTheme } from "$lib/utils/theme";

	let { children } = $props();

	let hostname = $state("");

	onMount(() => {
		initTheme();
		hostname = window.location.hostname;
	});

	const displayHost = $derived(
		hostname === "localhost" || hostname === "127.0.0.1"
			? "virtues.local"
			: hostname
	);
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

<div class="min-h-screen overscroll-none bg-surface flex items-center justify-center px-6">
	<div class="w-full max-w-sm">
		<div class="flex flex-col gap-1 mb-6">
			<p class="font-serif text-lg text-foreground-muted">
				{displayHost}
			</p>
		</div>

		<main>
			{@render children()}
		</main>
	</div>
</div>
