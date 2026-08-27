<script lang="ts">
	import "../../app.css";
	import { Toaster, toast } from "svelte-sonner";
	import "$lib/icons"; // Pre-load all icons
	import { UnifiedSidebar } from "$lib/components/sidebar";
	import { SplitContainer } from "$lib/components/tabs";
	import MobileShell from "$lib/components/mobile/MobileShell.svelte";
	import MobileOnboarding from "$lib/components/mobile/MobileOnboarding.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { ContextMenuProvider } from "$lib/components/contextMenu";
	import SearchModal from "$lib/components/sidebar/SearchModal.svelte";
	import { search } from "$lib/stores/search.svelte";
	import DialogHost from "$lib/components/DialogHost.svelte";
	import ServerProvisioning from "$lib/components/ServerProvisioning.svelte";
	import { FloatingContent } from "$lib/floating";
	import IconPicker from "$lib/components/IconPicker.svelte";
	import LinkEditorPopover from "$lib/components/pages/LinkEditorPopover.svelte";
	import { linkEditor } from "$lib/stores/linkEditor.svelte";
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
	import { reportBootOk, otaCheckNow } from "$lib/tauri/bridge";
	import { shortcuts } from "$lib/shortcuts/registry.svelte";
	import { modifierHint } from "$lib/stores/modifierHint.svelte";

	// @ts-ignore — Vite compile-time constant (see vite.config.ts + app.d.ts)
	const BUILD_COMMIT: string = __BUILD_COMMIT__;

	// Stamp X-Virtues-Client on box requests so this browser's build shows up on
	// the Devices page (update-manifold Phase 1). Idempotent, SSR-safe.
	installClientHeader();

	// Foreground OTA check — hoisted to component scope so onDestroy can remove
	// it. `onMount` is async here, so a returned cleanup would never run.
	function checkForNewUi() {
		if (!document.hidden) void otaCheckNow();
	}

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
		// Confirm to the shell that this build actually rendered. An OTA bundle
		// stays pending until this lands, and a bundle still pending at the next
		// launch is treated as one that failed to boot and is rolled back — so
		// removing this call silently reverts every update. It lives in onMount,
		// not at module scope, because a module that parses is not a page that
		// renders, and rendering is the thing being proven. See
		// src-tauri/src/web_bundle.rs.
		void reportBootOk();

		// Ask the shell to look for newer UI whenever we come back to the
		// foreground. The shell also checks at launch, but this app is not
		// relaunched often — the mic session keeps it alive for days — so
		// without this a phone could sit on a stale bundle indefinitely. The
		// check is cheap when there is nothing new (one small GET) and never
		// swaps the bundle underneath the running session; anything it applies
		// takes effect at the next launch.
		function checkForNewUi() {
			if (!document.hidden) void otaCheckNow();
		}
		document.addEventListener("visibilitychange", checkForNewUi);

		// Global dragover handler: Allow drops on document by preventing default
		// This is a fallback to ensure drops are never blocked by missing handlers.
		// The matching `drop` guard is in the ROOT layout — cancelling dragover
		// here is what makes the whole document a drop target, and an unclaimed
		// drop on that target navigates the window to the file. See
		// `routes/+layout.svelte`; the two belong together.
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

		// Window-level shortcuts. These live here rather than in the sidebar
		// because they're about panes and tabs, and the sidebar isn't mounted on
		// the phone shell.
		//
		// ⌘1-⌘9 address *tabs*, browser-style: leftmost tab is 1, counting
		// across both panes left-to-right. Activating a tab focuses its pane,
		// so pane switching falls out for free. Tab cycling takes ⌘⇧[ / ⌘⇧],
		// which is the browser convention and collides with nothing.
		shortcuts.register(
			...Array.from({ length: 9 }, (_, i) => ({
				id: `tab.focus-${i + 1}`,
				keys: `mod+${i + 1}`,
				label: `Go to tab ${i + 1}`,
				group: "Window",
				run: () => windowShellStore.activateTabByOrdinal(i + 1),
			})),
			{
				id: "tab.next",
				keys: "mod+shift+]",
				label: "Next tab",
				group: "Window",
				run: () => windowShellStore.cycleTab(1),
			},
			{
				id: "tab.previous",
				keys: "mod+shift+[",
				label: "Previous tab",
				group: "Window",
				run: () => windowShellStore.cycleTab(-1),
			},
		);

		// Hold-⌘ reveals the per-tab ⌘N badges.
		modifierHint.start();

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
							// Software, not the old catch-all Box — this toast is
							// about a version having changed, and that page is now
							// the one place that says which versions are in play.
							windowShellStore.openTabFromRoute("/virtues/software", {
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
		document.removeEventListener("visibilitychange", checkForNewUi);
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

<!-- Desktop: bottom-right, out of the way of the pane toolbar and the ⌘K modal.
     Mobile keeps top-center — it's the platform convention there, and the
     offset clears the notch/Dynamic Island on the edge-to-edge shell (env() is
     0 on desktop, so the desktop offset is the stock 16px gap). -->
<Toaster
	position={mobileLayout.isMobile ? "top-center" : "bottom-right"}
	offset="max(16px, env(safe-area-inset-bottom))"
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

<!-- Global confirm/prompt dialogs (replaces window.confirm/prompt) -->
<DialogHost />

<!-- Ask/search palette. Mounted here rather than inside UnifiedSidebar, which
     is where it used to live: the sidebar doesn't render on the phone shell,
     so the app's only way to find anything didn't exist there. -->
<SearchModal open={search.open} onClose={() => search.hide()} />

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
			{#if mobileLayout.isMobile}
				<!-- One window, no panes, no tabs — and only the visible view
				     is mounted. -->
				<MobileShell />
			{:else}
				<!-- SplitContainer handles both split and mono modes -->
				<SplitContainer />
			{/if}
		{/if}
	</main>
</div>

<!-- First-run stream setup (phone shell only). The shell itself carries all
     other mobile chrome — the drawer and the top bar live inside it. -->
{#if mobileLayout.isMobile}
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

<!-- Global icon picker.
     A popover, not a modal: the in-page and toolbar pickers have always been
     popovers, and the same panel arriving centred and dimming the document —
     to ask about one 16px glyph — read as a different, heavier feature. It
     hangs off wherever it was summoned from (a right-click point, a button
     rect); with no anchor it falls back to the middle of the window. -->
{#if iconPickerStore.open}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div class="icon-picker-scrim" onclick={() => iconPickerStore.hide()}></div>
	<FloatingContent
		anchor={iconPickerStore.anchor ?? {
			x: window.innerWidth / 2,
			y: window.innerHeight / 2,
			width: 0,
			height: 0,
		}}
		options={{ placement: "bottom-start", offset: 6, flip: true, shift: true }}
		class="icon-picker-floating"
	>
		{#snippet children()}
			<IconPicker
				value={iconPickerStore.currentValue}
				onSelect={(icon) => iconPickerStore.select(icon)}
				close={() => iconPickerStore.hide()}
				color={iconPickerStore.currentColor}
				onColorSelect={iconPickerStore.colorEnabled
					? (c) => iconPickerStore.selectColor(c)
					: undefined}
			/>
		{/snippet}
	</FloatingContent>
{/if}

<!-- Global link editor.
     Links render as links now, whether or not the caret is on them, so the raw
     `[label](url)` is never on screen to be corrected in place. This is where a
     label or a URL gets fixed instead. Anchored to the link it was opened from,
     same as the icon picker above. -->
{#if linkEditor.open}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div class="icon-picker-scrim" onclick={() => linkEditor.hide()}></div>
	<FloatingContent
		anchor={linkEditor.anchor ?? {
			x: window.innerWidth / 2,
			y: window.innerHeight / 2,
			width: 0,
			height: 0,
		}}
		options={{ placement: "bottom-start", offset: 6, flip: true, shift: true }}
		class="icon-picker-floating"
	>
		{#snippet children()}
			<LinkEditorPopover />
		{/snippet}
	</FloatingContent>
{/if}

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
	/* The bottom reservation is whichever is taller: the tab bar (plus the home
	   indicator), or the keyboard. They are never both owed — the bar hides
	   while the keyboard is up, and the home indicator is behind the keyboard
	   anyway — so `max()` is the whole rule. `--keyboard-inset` is 0 until
	   `stores/keyboard.svelte.ts` measures otherwise, which makes this
	   identical to what it was on every surface without a keyboard.

	   This is also what lifts the composer: the chat input sits in normal flow
	   at the bottom of its view, so shrinking main's content box moves it up
	   with the keyboard. No component needs to know it happened. */
	/* No reservation for the tab bar here, deliberately. The bar is glass, and
	   glass with nothing behind it is just a grey rectangle — reserving the
	   space meant the view stopped exactly where the bar began, so there was
	   never any content underneath to blur, which is the entire effect. The
	   view now runs to the bottom of the screen and its own scroller carries
	   the bottom padding instead (see Page.svelte), so the last row is still
	   reachable but everything above it passes behind the glass on its way up.

	   The keyboard inset stays: that one is not decoration, it is the
	   difference between seeing what you are typing and not. */
	main.is-mobile {
		padding-top: env(safe-area-inset-top);
		padding-bottom: var(--keyboard-inset, 0px);
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
		background: var(--hover-bg);
	}

	/* Invisible, not dim. The picker is a popover — the page behind it stays
	   readable and in context — but a click outside still has to dismiss it,
	   and a transparent full-screen catcher is how that works without every
	   caller wiring up click-outside itself. */
	.icon-picker-scrim {
		position: fixed;
		inset: 0;
		z-index: var(--z-popover, 1000);
		background: transparent;
	}

	/* Chrome only. The picker sizes and scrolls itself (360px, its own
	   max-height, its own overflow) — a second width and a second scroller out
	   here would fight it and produce two scrollbars. */
	:global(.icon-picker-floating) {
		z-index: calc(var(--z-popover, 1000) + 1);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		box-shadow: 0 12px 32px rgb(0 0 0 / 0.18);
	}
</style>
