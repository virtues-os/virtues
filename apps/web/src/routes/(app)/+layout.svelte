<script lang="ts">
	import "../../app.css";
	import { Toaster } from "svelte-sonner";
	import "iconify-icon";
	import { UnifiedSidebar } from "$lib/components/sidebar";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { onMount } from "svelte";
	import { createAIContext } from "@ai-sdk/svelte";
	import { initTheme } from "$lib/utils/theme";

	let { children } = $props();

	// Create AI context for synchronized state across Chat instances
	createAIContext();

	// Load chat sessions and initialize theme on mount
	onMount(() => {
		chatSessions.load();
		initTheme();
	});
</script>

<Toaster position="top-center" />

<div class="flex h-screen w-full">
	<!-- Unified Sidebar -->
	<UnifiedSidebar />

	<!-- Main Content -->
	<main class="flex-1 flex flex-col z-0 min-w-0 bg-surface text-foreground">
		<div class="flex-1 min-w-0 overflow-hidden">
			{@render children()}
		</div>
	</main>
</div>

<style>
	main {
		view-transition-name: main-content;
	}
</style>
