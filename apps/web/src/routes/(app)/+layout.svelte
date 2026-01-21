<script lang="ts">
	import "../../app.css";
	import { Toaster, toast } from "svelte-sonner";
	import "iconify-icon";
	import { UnifiedSidebar } from "$lib/components/sidebar";
	import { WindowTabBar, SplitContainer } from "$lib/components/tabs";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { windowTabs } from "$lib/stores/windowTabs.svelte";
	import { bookmarks } from "$lib/stores/bookmarks.svelte";
	import { onMount, onDestroy } from "svelte";
	import { createAIContext } from "@ai-sdk/svelte";
	import { initTheme } from "$lib/utils/theme";
	import { page } from "$app/state";
	import { goto } from "$app/navigation";

	// Get session expiry from page data
	const { data } = $props();
	let sessionExpiryTimer: ReturnType<typeof setInterval> | null = null;
	let warningShown = false;

	// Create AI context for synchronized state across Chat instances
	createAIContext();

	// Track if we're in initial load
	let initialized = $state(false);

	// Sync URL changes to tab system (for back/forward, direct navigation)
	$effect(() => {
		if (!initialized) return;
		// Skip if a programmatic tab change just occurred (prevents feedback loop)
		if (windowTabs.shouldSkipUrlSync()) {
			console.log('[Layout] Skipping URL sync (programmatic tab change)');
			return;
		}
		const route = page.url.pathname + page.url.search;
		console.log('[Layout] Syncing from URL:', route);
		windowTabs.syncFromUrl(route);
	});

	// Sync active tab to URL (for bookmarking)
	// Use history.replaceState directly to avoid triggering SvelteKit's load functions
	// This prevents full page navigation when switching tabs internally
	$effect(() => {
		if (!initialized) return;
		const activeRoute = windowTabs.activeRoute;
		if (activeRoute) {
			const currentRoute = window.location.pathname + window.location.search;
			if (activeRoute !== currentRoute) {
				console.log('[Layout] URL sync (shallow):', { from: currentRoute, to: activeRoute });
				// Update URL without triggering SvelteKit navigation
				history.replaceState(history.state, '', activeRoute);
			}
		}
	});

	// Load chat sessions, bookmarks, and initialize theme on mount
	onMount(() => {
		console.log('[Layout] onMount starting');
		chatSessions.load();
		bookmarks.load();
		initTheme();

		// Initialize the tab store (restores from localStorage)
		console.log('[Layout] Calling windowTabs.init()');
		windowTabs.init();
		console.log('[Layout] After init, getAllTabs().length:', windowTabs.getAllTabs().length);

		// Initialize from current URL if no tabs exist (check both split and non-split modes)
		const route = window.location.pathname + window.location.search;
		if (windowTabs.getAllTabs().length === 0) {
			console.log('[Layout] No tabs, opening from route:', route);
			windowTabs.openTabFromRoute(route);
		}

		// Mark as initialized after first render
		initialized = true;
		console.log('[Layout] onMount complete');

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
						description: "You'll be logged out soon. Save your work.",
						duration: 30000
					});
				}

				// If session has expired, redirect to login
				if (timeLeft <= 0) {
					toast.error('Session expired', {
						description: 'Please log in again.'
					});
					goto('/login');
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

<Toaster position="top-center" />

<div class="flex h-screen w-full bg-surface-elevated">
	<!-- Unified Sidebar -->
	<UnifiedSidebar />

	<!-- Main Content -->
	<main class="flex-1 flex flex-col z-0 min-w-0 bg-surface text-foreground m-3 rounded-lg border border-border overflow-hidden">
		{#if windowTabs.split.enabled}
			<!-- Split view mode - each pane has its own tab bar -->
			<SplitContainer />
		{:else}
			<!-- Single pane mode -->
			<WindowTabBar />
			<SplitContainer />
		{/if}
	</main>
</div>

<style>
	main {
		view-transition-name: main-content;
	}
</style>
