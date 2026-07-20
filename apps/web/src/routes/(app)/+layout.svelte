<script lang="ts">
	import "../../app.css";
	import { Toaster, toast } from "svelte-sonner";
	import "$lib/icons"; // Pre-load all icons
	import { UnifiedSidebar } from "$lib/components/sidebar";
	import { SplitContainer } from "$lib/components/tabs";
	import MobileTabBar from "$lib/components/mobile/MobileTabBar.svelte";
	import MobileSettingsView from "$lib/components/mobile/MobileSettingsView.svelte";
	import MobileOnboarding from "$lib/components/mobile/MobileOnboarding.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { ContextMenuProvider } from "$lib/components/contextMenu";
	import ServerProvisioning from "$lib/components/ServerProvisioning.svelte";
	import Modal from "$lib/components/Modal.svelte";
	import IconPicker from "$lib/components/IconPicker.svelte";
	import { iconPickerStore } from "$lib/stores/iconPicker.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { pinsStore } from "$lib/stores/pins.svelte";
	import { notebookStore } from "$lib/stores/notebook.svelte";
	import { subscriptionStore } from "$lib/stores/subscription.svelte";
	import { setupStateStore } from "$lib/stores/setupState.svelte";
	import { sidebarState } from "$lib/stores/sidebarState.svelte";
	import { pageDisplay } from "$lib/stores/pageDisplay.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { onMount, onDestroy } from "svelte";
	import { createAIContext } from "@ai-sdk/svelte";
	import { initTheme } from "$lib/utils/theme";
	import { goto } from "$app/navigation";
	import { page } from "$app/stores";
	import type { Snippet } from "svelte";

	import { installClientHeader } from "$lib/build";

	// @ts-ignore — Vite compile-time constant (see vite.config.ts + app.d.ts)
	const BUILD_COMMIT: string = __BUILD_COMMIT__;

	// Stamp X-Virtues-Client on box requests so this browser's build shows up on
	// the Devices page (update-manifold Phase 1). Idempotent, SSR-safe.
	installClientHeader();

	// Get session expiry from page data
	// Note: children is intentionally not rendered - this app uses a custom tab-based routing system
	const { data, children }: { data: any; children: Snippet } = $props();
	let sessionExpiryTimer: ReturnType<typeof setInterval> | null = null;
	let warningShown = false;

	// Create AI context for synchronized state across Chat instances
	createAIContext();

	// Track initialization state
	let initialized = $state(false);

	// Full-screen focus mode: hide all app chrome (sidebar, tab bars, frame)
	// by toggling a body class that app.css keys off. Driven by the same
	// pageDisplay.focusMode that the editor's dim/typewriter mode uses.
	$effect(() => {
		document.body.classList.toggle("focus-mode", pageDisplay.focusMode);
		return () => document.body.classList.remove("focus-mode");
	});

	// Load chat sessions, workspaces, and initialize theme on mount
	onMount(async () => {
		// Global dragover handler: Allow drops on document by preventing default
		// This is a fallback to ensure drops are never blocked by missing handlers
		document.addEventListener("dragover", (e) => {
			e.preventDefault();
			if (e.dataTransfer) {
				e.dataTransfer.dropEffect = "move";
			}
		});

		// Collapse sidebar for new users (first-time onboarding)
		if (data?.onboardingStatus === 'new') {
			sidebarState.collapsed = true;
		}

		// Load global data
		chatSessions.load();
		pinsStore.load();
		notebookStore.load();
		initTheme();

		// Initialize workspace store (loads workspaces, tree, and tabs)
		await windowShellStore.init();

		// Handle deep link from URL (e.g., /pages/page_abc123 or /wiki/rome)
		// Note: searchParams.get() already decodes the value, no need for decodeURIComponent
		const urlPath = $page.url.pathname;
		const rightParam = $page.url.searchParams.get("right");
		// Preserve route-level params (e.g. ?page=N for the PDF viewer) —
		// only ?right= belongs to the shell itself.
		const routeParams = new URLSearchParams($page.url.searchParams);
		routeParams.delete("right");
		const routeWithParams =
			routeParams.size > 0 ? `${urlPath}?${routeParams}` : urlPath;
		windowShellStore.handleDeepLink(routeWithParams, rightParam);

		// Enable URL sync for future navigation
		windowShellStore.initUrlSync();

		// Mark as initialized
		initialized = true;

		// Start polling for subscription status
		subscriptionStore.start();

		// Start polling for setup/onboarding state (next-wins checklist,
		// remote-access flip toast). Stops itself once everything is done.
		setupStateStore.start();

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
							windowShellStore.openTabFromRoute("/virtues/box", {
								label: "Settings",
								preferEmptyPane: true,
							}),
					},
				});
			}
			sessionStorage.setItem("virtues_last_commit", BUILD_COMMIT);
		}

		// NOTE: home_timezone (the box's location) is server-sourced and NOT
		// browser-tracked — see docs/timezone-model.md. The browser's zone is sent
		// per-request via ?tz= for the live "today" view (getDaySources), so there
		// is no profile write-through here.

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
					goto("/pair");
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
		windowShellStore.destroyUrlSync();
		subscriptionStore.stop();
		setupStateStore.stop();

		// (workspace switching keyboard shortcuts removed — single workspace now)
	});

	// Remote-access flip toast: the verdict flipped to reachable mid-session.
	// Session-only by design (no persistence) — the flag is consumed so it
	// fires exactly once per flip; first-load is suppressed by the store.
	$effect(() => {
		if (setupStateStore.remoteAccessFlipped) {
			setupStateStore.remoteAccessFlipped = false;
			toast.success("Your box is now reachable from anywhere", {
				description: setupStateStore.remoteAccess?.detail,
			});
		}
	});

	// Trial countdown toasts (day 5, 2, 1, 0)
	let trialToastShownForDay: number | null = null;
	$effect(() => {
		const days = subscriptionStore.daysRemaining;
		if (days === null || subscriptionStore.status !== "trialing") return;
		if (trialToastShownForDay === days) return;

		const openBilling = () =>
			windowShellStore.openTabFromRoute("/virtues/billing", {
				label: "Settings",
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
						windowShellStore.openTabFromRoute("/virtues/billing", {
							label: "Settings",
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

<!-- offset clears the notch/Dynamic Island on the edge-to-edge mobile shell
     (env() is 0 on desktop, so this is the stock 16px gap there). -->
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
		class: "themed-toast",
	}}
/>

<!-- Global Context Menu Provider -->
<ContextMenuProvider />

<div
	class="app-shell flex h-screen w-full bg-surface-elevated"
	class:mobile-shell={mobileLayout.isMobile}
	style="background-image: var(--surface-elevated-image); background-size: var(--surface-elevated-size);"
>
	<!-- Desktop sidebar — hidden on the mobile (bottom-tab) shell -->
	{#if !mobileLayout.isMobile}
		<UnifiedSidebar />
	{/if}

	<!-- Main Content -->
	<main
		class="flex-1 flex flex-col z-0 min-w-0 text-foreground overflow-hidden
			transition-[border-color,background-color] duration-150"
		class:m-3={!mobileLayout.isMobile}
		class:border={!mobileLayout.isMobile}
		class:rounded-lg={!mobileLayout.isMobile}
		class:is-mobile={mobileLayout.isMobile}
		class:bg-surface={!windowShellStore.isSplit}
		class:bg-transparent={windowShellStore.isSplit}
		class:border-border={!windowShellStore.isSplit && !mobileLayout.isMobile}
		class:border-transparent={windowShellStore.isSplit}
		style="background-image: {windowShellStore.isSplit ? 'none' : 'var(--background-image)'}; background-blend-mode: multiply;"
	>
		{#if initialized}
			<!-- SplitContainer handles both split and mono modes -->
			<SplitContainer />
		{/if}
	</main>
</div>

<!-- Mobile bottom-tab chrome (phone shell only) -->
{#if mobileLayout.isMobile}
	<MobileTabBar />
	<MobileSettingsView />
	<MobileOnboarding />
{/if}

<!-- Focus mode: floating exit affordance (chrome is hidden via body.focus-mode) -->
{#if pageDisplay.focusMode}
	<button
		class="focus-exit"
		onclick={() => pageDisplay.toggleFocus()}
		title="Exit focus mode (⌘⇧F)"
		aria-label="Exit focus mode"
	>
		<Icon icon="ri:fullscreen-exit-line" width="18" />
	</button>
{/if}

<!-- Server Provisioning Overlay (shown while virtues-api is hydrating) -->
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
{#if false}
	{@render children()}
{/if}

<style>
	main {
		view-transition-name: main-content;
	}

	/* Mobile shell: edge-to-edge (viewport-fit=cover), so the shell itself pads
	   for the status bar / Dynamic Island, and reserves the bottom-tab bar's
	   height so scrollable content ends above it (the bar is position:fixed).
	   The padded zones show the themed shell background instead of the bare
	   native window. */
	main.is-mobile {
		padding-top: env(safe-area-inset-top);
		padding-bottom: calc(50px + env(safe-area-inset-bottom));
	}

	/* Pin the whole shell to the viewport on mobile. Without this, iOS lets the
	   WKWebView scroll the *document* — which drags the position:fixed bottom bar
	   and the top tab strip out of view. Fixed + inset:0 takes the shell out of
	   normal flow (height comes from the insets, overriding h-screen's 100vh), so
	   the document has nothing to scroll; only the view's own inner scroller
	   moves. Bars stay put. overflow-x:hidden also kills sideways rubber-banding. */
	.app-shell.mobile-shell {
		position: fixed;
		inset: 0;
		height: auto;
		width: auto;
		overflow: hidden;
		overscroll-behavior: none;
	}

	/* Focus-mode exit button — appears top-right when chrome is hidden. */
	.focus-exit {
		position: fixed;
		top: max(1rem, env(safe-area-inset-top));
		right: max(1rem, env(safe-area-inset-right));
		z-index: var(--z-modal);
		display: flex;
		align-items: center;
		justify-content: center;
		width: 34px;
		height: 34px;
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 8px;
		background: var(--color-surface);
		color: var(--color-foreground-muted);
		cursor: pointer;
		opacity: 0.4;
		transition:
			opacity 0.18s ease,
			color 0.15s ease,
			background-color 0.15s ease;
	}

	.focus-exit:hover {
		opacity: 1;
		color: var(--color-foreground);
		background: var(--color-surface-elevated);
	}
</style>
