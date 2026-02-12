<script lang="ts">
	import "../../app.css";
	import { Toaster, toast } from "svelte-sonner";
	import "$lib/icons"; // Pre-load all icons
	import { UnifiedSidebar } from "$lib/components/sidebar";
	import { SplitContainer } from "$lib/components/tabs";
	import { ContextMenuProvider } from "$lib/components/contextMenu";
	import ServerProvisioning from "$lib/components/ServerProvisioning.svelte";
	import Modal from "$lib/components/Modal.svelte";
	import IconPicker from "$lib/components/IconPicker.svelte";
	import { iconPickerStore } from "$lib/stores/iconPicker.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import { subscriptionStore } from "$lib/stores/subscription.svelte";
	import { onMount, onDestroy } from "svelte";
	import { createAIContext } from "@ai-sdk/svelte";
	import { initTheme } from "$lib/utils/theme";
	import { goto } from "$app/navigation";
	import { page } from "$app/stores";
	import type { Snippet } from "svelte";

	// @ts-ignore — Vite compile-time constant (see vite.config.ts + app.d.ts)
	const BUILD_COMMIT: string = __BUILD_COMMIT__;

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

	// Global keyboard shortcut handler for workspace switching (⌘1-9)
	function handleGlobalKeydown(e: KeyboardEvent) {
		// ⌘1-9 for workspace switching
		if (e.metaKey && e.key >= "1" && e.key <= "9") {
			e.preventDefault();
			const index = parseInt(e.key) - 1;
			const spaces = spaceStore.spaces;
			if (index < spaces.length) {
				spaceStore.switchSpace(spaces[index].id, true);
			}
		}
	}

	// Load chat sessions, workspaces, and initialize theme on mount
	onMount(async () => {
		// Register global keyboard shortcuts
		window.addEventListener("keydown", handleGlobalKeydown);
		// Global dragover handler: Allow drops on document by preventing default
		// This is a fallback to ensure drops are never blocked by missing handlers
		document.addEventListener("dragover", (e) => {
			e.preventDefault();
			if (e.dataTransfer) {
				e.dataTransfer.dropEffect = "move";
			}
		});

		// Load global data
		chatSessions.load();
		initTheme();

		// Initialize workspace store (loads workspaces, tree, and tabs)
		await spaceStore.init();

		// Handle deep link from URL (e.g., /pages/page_abc123 or /wiki/rome)
		// Note: searchParams.get() already decodes the value, no need for decodeURIComponent
		const urlPath = $page.url.pathname;
		const rightParam = $page.url.searchParams.get("right");
		spaceStore.handleDeepLink(urlPath, rightParam);

		// Enable URL sync for future navigation
		spaceStore.initUrlSync();

		// Mark as initialized
		initialized = true;

		// Start polling for subscription status
		subscriptionStore.start();

		// Post-update toast: show once per session if the server was updated
		if (typeof sessionStorage !== "undefined") {
			const lastSeenCommit = sessionStorage.getItem(
				"virtues_last_commit",
			);
			if (
				BUILD_COMMIT !== "dev" &&
				lastSeenCommit &&
				lastSeenCommit !== BUILD_COMMIT
			) {
				toast.info("Virtues has been updated", {
					description: BUILD_COMMIT.slice(0, 7),
					duration: 8000,
					action: {
						label: "Details",
						onClick: () =>
							spaceStore.openTabFromRoute("/virtues/system", {
								label: "System",
								preferEmptyPane: true,
							}),
					},
				});
			}
			sessionStorage.setItem("virtues_last_commit", BUILD_COMMIT);
		}

		// Timezone auto-detect: silently set on first launch, toast on mismatch
		const browserTz = Intl.DateTimeFormat().resolvedOptions().timeZone;
		if (!data?.profileTimezone) {
			fetch("/api/profile", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ timezone: browserTz }),
			}).catch((e) => console.error("[Layout] Failed to set timezone:", e));
		} else if (data.profileTimezone !== browserTz) {
			const formatTz = (tz: string) => tz.replace(/_/g, " ");
			toast.info("Timezone changed?", {
				description: `Browser: ${formatTz(browserTz)} · Profile: ${formatTz(data.profileTimezone)}`,
				duration: 15000,
				action: {
					label: "Update",
					onClick: () => {
						fetch("/api/profile", {
							method: "PUT",
							headers: { "Content-Type": "application/json" },
							body: JSON.stringify({ timezone: browserTz }),
						})
							.then(() => toast.success("Timezone updated"))
							.catch((e) => console.error("Failed to update timezone:", e));
					},
				},
			});
		}

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
		spaceStore.destroyUrlSync();
		subscriptionStore.stop();

		// Clean up global keyboard shortcut listener
		if (typeof window !== "undefined") {
			window.removeEventListener("keydown", handleGlobalKeydown);
		}
	});

	// Trial countdown toasts (day 5, 2, 1, 0)
	let trialToastShownForDay: number | null = null;
	$effect(() => {
		const days = subscriptionStore.daysRemaining;
		if (days === null || subscriptionStore.status !== "trialing") return;
		if (trialToastShownForDay === days) return;

		const openBilling = () =>
			spaceStore.openTabFromRoute("/virtues/billing", {
				label: "Billing",
				preferEmptyPane: true,
			});

		if (days <= 5 && days > 2) {
			trialToastShownForDay = days;
			toast.warning(`Trial ends in ${days} days`, {
				description: "Add a payment method to keep your data.",
				duration: Infinity,
				action: { label: "Billing", onClick: openBilling },
			});
		} else if (days <= 2 && days > 0) {
			trialToastShownForDay = days;
			toast.error(`Trial ends in ${days} day${days === 1 ? "" : "s"}`, {
				description: "Your instance will be suspended without payment.",
				duration: Infinity,
				action: { label: "Add Payment", onClick: openBilling },
			});
		} else if (days <= 0) {
			trialToastShownForDay = days;
			toast.error("Trial expired", {
				description: "Add a payment method to restore access.",
				duration: Infinity,
				action: { label: "Add Payment", onClick: openBilling },
			});
		}
	});

	// Show toast when subscription is expired (from 402 or polling)
	let expiredToastShown = false;
	$effect(() => {
		if (
			!subscriptionStore.isActive &&
			subscriptionStore.status === "expired" &&
			!expiredToastShown
		) {
			expiredToastShown = true;
			toast.error("Subscription required", {
				description:
					"Your trial has ended. Subscribe to continue using AI features.",
				duration: Infinity,
				action: {
					label: "Subscribe",
					onClick: () =>
						spaceStore.openTabFromRoute("/virtues/billing", {
							label: "Billing",
							preferEmptyPane: true,
						}),
				},
			});
		}
		// Reset if subscription becomes active again
		if (subscriptionStore.isActive) {
			expiredToastShown = false;
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
		class: "themed-toast",
	}}
/>

<!-- Global Context Menu Provider -->
<ContextMenuProvider />

<div
	class="app-shell flex h-screen w-full bg-surface-elevated"
	style="background-image: var(--surface-elevated-image); background-size: var(--surface-elevated-size);"
>
	<!-- Unified Sidebar -->
	<UnifiedSidebar />

	<!-- Main Content -->
	<main
		class="flex-1 flex flex-col z-0 min-w-0 text-foreground m-3 overflow-hidden
			border rounded-lg transition-[border-color,background-color] duration-150"
		class:bg-surface={!spaceStore.isSplit}
		class:bg-transparent={spaceStore.isSplit}
		class:border-border={!spaceStore.isSplit}
		class:border-transparent={spaceStore.isSplit}
		style="background-image: {spaceStore.isSplit ? 'none' : 'var(--background-image)'}; background-blend-mode: multiply;"
	>
		{#if initialized}
			<!-- SplitContainer handles both split and mono modes -->
			<SplitContainer />
		{/if}
	</main>
</div>

<!-- Server Provisioning Overlay (shown while Tollbooth is hydrating) -->
{#if data?.serverStatus && data.serverStatus !== "ready"}
	<ServerProvisioning initialStatus={data.serverStatus} />
{/if}

<!-- Global Icon Picker Modal -->
<Modal open={iconPickerStore.open} onClose={() => iconPickerStore.hide()} title="Change Icon" width="md">
	{#snippet children()}
		<IconPicker
			value={iconPickerStore.currentValue}
			onSelect={(icon) => iconPickerStore.select(icon)}
			close={() => iconPickerStore.hide()}
		/>
	{/snippet}
</Modal>

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
