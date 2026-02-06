<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { page } from "$app/state";
	import { goto } from "$app/navigation";
	import { spaceStore } from "$lib/stores/space.svelte";
	import SidebarTooltip from "./SidebarTooltip.svelte";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let {
		collapsed = false,
		animationDelay = 0,
	}: Props = $props();

	// Settings folder expansion state
	let isSettingsExpanded = $state(false);

	// Check if profiles is the active route
	const isProfilesActive = $derived(
		page.url.pathname.startsWith("/virtues/account") ||
		page.url.pathname.startsWith("/virtues/assistant") ||
			spaceStore.activeTab?.type === "virtues",
	);

	function toggleSettings() {
		isSettingsExpanded = !isSettingsExpanded;
	}

	function handleProfilesClick(e: MouseEvent) {
		e.preventDefault();
		spaceStore.openTabFromRoute("/virtues/account", {
			label: "Profile",
			preferEmptyPane: true,
		});
	}

	function handleAssistantClick(e: MouseEvent) {
		e.preventDefault();
		spaceStore.openTabFromRoute("/virtues/assistant", {
			label: "Assistant",
			preferEmptyPane: true,
		});
	}

	function handleBillingClick(e: MouseEvent) {
		e.preventDefault();
		spaceStore.openTabFromRoute("/virtues/billing", {
			label: "Billing",
			preferEmptyPane: true,
		});
	}

	function handleChangelogClick(e: MouseEvent) {
		e.preventDefault();
		spaceStore.openTabFromRoute("/virtues/changelog", {
			label: "What's New",
			preferEmptyPane: true,
		});
	}

	function handleFeedbackClick(e: MouseEvent) {
		e.preventDefault();
		spaceStore.openTabFromRoute("/virtues/feedback", {
			label: "Feedback",
			preferEmptyPane: true,
		});
	}

	async function handleLogout() {
		try {
			const response = await fetch("/auth/signout", { method: "POST" });
			if (response.ok) {
				// Clear any client-side state
				spaceStore.closeAllTabs();
				// Redirect to login
				await goto("/login");
			} else {
				console.error("[Logout] Failed to sign out:", response.status);
				// Even if signout fails, redirect to login to clear session
				await goto("/login");
			}
		} catch (error) {
			console.error("[Logout] Error:", error);
			// On network error, still redirect to login
			await goto("/login");
		}
	}
</script>

<div
	class="footer"
	class:collapsed
	style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
>
	{#if collapsed}
		<!-- Collapsed: just show settings icon -->
		<SidebarTooltip content="Settings">
			<button
				onclick={toggleSettings}
				class="sidebar-interactive collapsed"
				class:active={isProfilesActive}
				aria-label="Settings"
				title="Settings"
			>
				<Icon icon="ri:settings-4-line" width="18" />
			</button>
		</SidebarTooltip>
	{:else}
		<!-- Settings folder header -->
		<button
			onclick={toggleSettings}
			class="sidebar-interactive"
			class:active={isProfilesActive}
		>
			<span class="sidebar-label">Settings</span>
			<svg
				class="sidebar-chevron"
				class:expanded={isSettingsExpanded}
				width="10"
				height="10"
				viewBox="0 0 12 12"
				fill="none"
			>
				<path
					d="M4.5 3L7.5 6L4.5 9"
					stroke="currentColor"
					stroke-width="1.5"
					stroke-linecap="round"
					stroke-linejoin="round"
				/>
			</svg>
		</button>

		<!-- Settings children (expandable) -->
		<div class="sidebar-expandable" class:expanded={isSettingsExpanded}>
			<div class="sidebar-expandable-inner children-inner">
				<button
					onclick={handleProfilesClick}
					class="sidebar-interactive"
					class:active={page.url.pathname.startsWith("/virtues/account")}
				>
					<Icon icon="ri:user-3-line" width="16" class="sidebar-icon" />
					<span class="sidebar-label">Profile</span>
				</button>

				<button
					onclick={handleAssistantClick}
					class="sidebar-interactive"
					class:active={page.url.pathname.startsWith("/virtues/assistant")}
				>
					<Icon icon="ri:robot-line" width="16" class="sidebar-icon" />
					<span class="sidebar-label">Assistant</span>
				</button>

				<button
					onclick={handleBillingClick}
					class="sidebar-interactive"
					class:active={page.url.pathname.startsWith("/virtues/billing")}
				>
					<Icon icon="ri:bank-card-line" width="16" class="sidebar-icon" />
					<span class="sidebar-label">Billing</span>
				</button>

				<button
					onclick={handleChangelogClick}
					class="sidebar-interactive"
					class:active={page.url.pathname.startsWith("/virtues/changelog")}
				>
					<Icon icon="ri:megaphone-line" width="16" class="sidebar-icon" />
					<span class="sidebar-label">What's New</span>
				</button>

				<button
					onclick={handleFeedbackClick}
					class="sidebar-interactive"
					class:active={page.url.pathname.startsWith("/virtues/feedback")}
				>
					<Icon icon="ri:feedback-line" width="16" class="sidebar-icon" />
					<span class="sidebar-label">Feedback</span>
				</button>

				<button onclick={handleLogout} class="sidebar-interactive logout-btn">
					<Icon icon="ri:logout-box-r-line" width="16" class="sidebar-icon" />
					<span class="sidebar-label">Sign Out</span>
				</button>
			</div>
		</div>
	{/if}
</div>

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	.footer {
		@apply flex flex-col gap-1 py-3 mt-auto;
		padding-left: 8px;
		/* Staggered load animation (initial mount) */
		animation: sidebar-fade-slide-in 200ms var(--sidebar-transition-easing) backwards;
		/* Staggered expand transition - uses --stagger-delay CSS var */
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--sidebar-transition-easing) var(--stagger-delay, 0ms),
			transform 200ms var(--sidebar-transition-easing) var(--stagger-delay, 0ms);
	}

	.footer.collapsed {
		@apply items-center;
		padding-left: 4px;
		padding-right: 4px;
		opacity: 0;
		transition:
			opacity var(--sidebar-transition-duration) var(--sidebar-transition-easing),
			transform var(--sidebar-transition-duration) var(--sidebar-transition-easing);
	}

	/* Children indent - uses shared sidebar variable */
	.children-inner {
		padding-left: var(--sidebar-indent-width);
	}

	/* Logout button special hover state */
	.sidebar-interactive.logout-btn:hover {
		background: color-mix(in srgb, var(--color-error) 15%, transparent);
		color: var(--color-error);
	}

	.sidebar-interactive.logout-btn:hover :global(.sidebar-icon) {
		color: var(--color-error);
	}
	/* Base icon styles are in sidebar.css (globally imported in app.css) */
</style>
