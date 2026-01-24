<script lang="ts">
	import "../../app.css";
	import { Toaster, toast } from "svelte-sonner";
	import "iconify-icon";
	import { UnifiedSidebar } from "$lib/components/sidebar";
	import { SplitContainer } from "$lib/components/tabs";
	import ServerProvisioning from "$lib/components/ServerProvisioning.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import { bookmarks } from "$lib/stores/bookmarks.svelte";
	import { onMount, onDestroy } from "svelte";
	import { createAIContext } from "@ai-sdk/svelte";
	import { initTheme } from "$lib/utils/theme";
	import { goto } from "$app/navigation";
	import type { Snippet } from "svelte";

	// Get session expiry from page data
	// Note: children is intentionally not rendered - this app uses a custom tab-based routing system
	// svelte-ignore slot_snippet_conflict
	const { data, children }: { data: any; children: Snippet } = $props();
	let sessionExpiryTimer: ReturnType<typeof setInterval> | null = null;
	let warningShown = false;

	// Create AI context for synchronized state across Chat instances
	createAIContext();

	// Track initialization state
	let initialized = $state(false);

	// Load chat sessions, bookmarks, workspaces, and initialize theme on mount
	onMount(async () => {
		// Load global data
		chatSessions.load();
		bookmarks.load();
		initTheme();

		// Initialize workspace store (loads workspaces, tree, and tabs)
		await workspaceStore.init();

		// Mark as initialized
		initialized = true;

		// Set up session expiry warning
		if (data?.sessionExpires) {
			const checkSessionExpiry = () => {
				const expires = new Date(data.sessionExpires).getTime();
				const now = Date.now();
				const timeLeft = expires - now;
				const oneHour = 60 * 60 * 1000;

				// Show warning when less than 1 hour remaining
				if (timeLeft > 0 && timeLeft < oneHour && !warningShown) {
					warningShown = true;
					const minutesLeft = Math.round(timeLeft / 60000);
					toast.warning(`Session expires in ${minutesLeft} minutes`, {
						description:
							"You'll be logged out soon. Save your work.",
						duration: 30000,
					});
				}

				// If session has expired, redirect to login
				if (timeLeft <= 0) {
					toast.error("Session expired", {
						description: "Please log in again.",
					});
					goto("/login");
				}
			};

			// Check immediately and then every 5 minutes
			checkSessionExpiry();
			sessionExpiryTimer = setInterval(checkSessionExpiry, 5 * 60 * 1000);
		}
	});

	onDestroy(() => {
		if (sessionExpiryTimer) {
			clearInterval(sessionExpiryTimer);
		}
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
		class: 'themed-toast',
	}}
/>

<div class="flex h-screen w-full bg-surface-elevated">
	<!-- Unified Sidebar -->
	<UnifiedSidebar />

	<!-- Main Content -->
	<main
		class="flex-1 flex flex-col z-0 min-w-0 bg-surface text-foreground m-3 rounded-lg border border-border overflow-hidden"
	>
		<!-- SplitContainer handles both split and mono modes -->
		<SplitContainer />
	</main>
</div>

<!-- Server Provisioning Overlay (shown while Tollbooth is hydrating) -->
{#if data?.serverStatus && data.serverStatus !== "ready"}
	<ServerProvisioning initialStatus={data.serverStatus} />
{/if}

<!-- Hidden: SvelteKit children are not rendered - using custom tab-based routing instead -->
<!-- svelte-ignore slot_snippet_conflict -->
{#if false}
	{@render children()}
{/if}

<style>
	main {
		view-transition-name: main-content;
	}
</style>
