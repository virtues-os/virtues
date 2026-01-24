<script lang="ts">
	import "iconify-icon";
	import { page } from "$app/state";
	import { goto } from "$app/navigation";
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import SidebarTooltip from "./SidebarTooltip.svelte";
	import WorkspaceSwitcher from "./WorkspaceSwitcher.svelte";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let {
		collapsed = false,
		animationDelay = 0,
	}: Props = $props();

	// Profile folder expansion state
	let isProfileExpanded = $state(false);

	// Check if settings is the active route
	const isSettingsActive = $derived(
		page.url.pathname.startsWith("/profile") ||
			workspaceStore.activeTab?.type === "profile",
	);

	function toggleProfile() {
		isProfileExpanded = !isProfileExpanded;
	}

	function handleSettingsClick(e: MouseEvent) {
		e.preventDefault();
		workspaceStore.openTabFromRoute("/profile/account", {
			label: "Settings",
			preferEmptyPane: true,
		});
	}

	function handleFeedbackClick(e: MouseEvent) {
		e.preventDefault();
		workspaceStore.openTabFromRoute("/feedback", {
			label: "Feedback",
			preferEmptyPane: true,
		});
	}

	async function handleLogout() {
		try {
			const response = await fetch("/auth/signout", { method: "POST" });
			if (response.ok) {
				// Clear any client-side state
				workspaceStore.closeAllTabs();
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
		<!-- Collapsed: just show profile icon -->
		<SidebarTooltip content="Profile">
			<button
				onclick={toggleProfile}
				class="footer-icon-btn"
				class:active={isSettingsActive}
				aria-label="Profile"
				title="Profile"
			>
				<iconify-icon icon="ri:user-3-line" width="18"></iconify-icon>
			</button>
		</SidebarTooltip>
	{:else}
		<!-- Profile folder header -->
		<button
			onclick={toggleProfile}
			class="profile-header"
			class:expanded={isProfileExpanded}
			class:active={isSettingsActive}
		>
			<iconify-icon icon="ri:user-3-line" width="16" class="nav-icon"></iconify-icon>
			<span class="nav-label">Profile</span>
			<svg
				class="chevron"
				class:expanded={isProfileExpanded}
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

		<!-- Profile children (expandable) -->
		<div class="profile-children" class:expanded={isProfileExpanded}>
			<div class="children-inner">
				<button
					onclick={handleSettingsClick}
					class="user-card"
					class:active={isSettingsActive}
				>
					<iconify-icon icon="ri:settings-4-line" width="16" class="nav-icon"></iconify-icon>
					<span class="nav-label">Settings</span>
				</button>

				<button
					onclick={handleFeedbackClick}
					class="user-card"
					class:active={page.url.pathname.startsWith("/feedback")}
				>
					<iconify-icon icon="ri:feedback-line" width="16" class="nav-icon"></iconify-icon>
					<span class="nav-label">Feedback</span>
				</button>

				<button onclick={handleLogout} class="user-card logout-btn">
					<iconify-icon icon="ri:logout-box-r-line" width="16" class="nav-icon"></iconify-icon>
					<span class="nav-label">Sign Out</span>
				</button>
			</div>
		</div>
	{/if}

	<!-- Workspace Switcher (always at bottom, not part of profile) -->
	{#if !collapsed}
		<WorkspaceSwitcher />
	{/if}
</div>

<style>
	@reference "../../../app.css";

	/* Premium easing - heavy friction feel */
	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	/* Staggered fade-slide animation with premium easing */
	@keyframes fadeSlideIn {
		from {
			opacity: 0;
			transform: translateX(-8px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	.footer {
		@apply flex flex-col gap-1 py-3 mt-auto;
		padding-left: 8px;
		padding-right: 8px;
		/* Staggered load animation (initial mount) */
		animation: fadeSlideIn 200ms var(--ease-premium) backwards;
		/* Staggered expand transition - uses --stagger-delay CSS var */
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--ease-premium) var(--stagger-delay, 0ms),
			transform 200ms var(--ease-premium) var(--stagger-delay, 0ms);
	}

	.footer.collapsed {
		@apply items-center;
		padding-left: 4px;
		padding-right: 4px;
		opacity: 0;
		transition:
			opacity 150ms var(--ease-premium),
			transform 150ms var(--ease-premium);
	}

	/* Profile folder header */
	.profile-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 10px;
		border-radius: 8px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 13px;
		border: none;
		cursor: pointer;
		width: 100%;
		text-align: left;
		transition:
			background-color 200ms var(--ease-premium),
			color 200ms var(--ease-premium);
	}

	.profile-header:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
	}

	.profile-header.active {
		color: var(--color-foreground);
	}

	.profile-header .chevron {
		margin-left: auto;
		color: var(--color-foreground-subtle);
		opacity: 0.5;
		transition: transform 150ms var(--ease-premium), opacity 150ms var(--ease-premium);
	}

	.profile-header:hover .chevron {
		opacity: 1;
	}

	.profile-header .chevron.expanded {
		transform: rotate(90deg);
		opacity: 0.8;
	}

	/* Profile children expansion */
	.profile-children {
		display: grid;
		grid-template-rows: 0fr;
		transition: grid-template-rows 200ms var(--ease-premium);
		overflow: hidden;
	}

	.profile-children.expanded {
		grid-template-rows: 1fr;
	}

	.profile-children .children-inner {
		min-height: 0;
		overflow: hidden;
		padding-left: 12px;
	}

	/* User card - matches nav-item pill style exactly */
	.user-card {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 10px;
		border-radius: 8px;
		text-decoration: none;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 13px;
		border: none;
		cursor: pointer;
		width: 100%;
		text-align: left;
		transition:
			background-color 200ms var(--ease-premium),
			color 200ms var(--ease-premium);
	}

	.user-card:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
	}

	/* Active state - zinc shadow style matching nav items */
	.user-card.active {
		background: color-mix(in srgb, var(--color-foreground) 9%, transparent);
		color: var(--color-foreground);
		font-weight: 500;
	}

	/* Logout button hover state */
	.user-card.logout-btn:hover,
	.footer-icon-btn.logout-btn:hover {
		background: color-mix(in srgb, var(--color-error) 15%, transparent);
		color: var(--color-error);
	}

	.user-card.logout-btn:hover .nav-icon,
	.footer-icon-btn.logout-btn:hover {
		color: var(--color-error);
	}

	/* Icon - matches nav-item icon style */
	.nav-icon {
		flex-shrink: 0;
		color: var(--color-foreground-subtle);
		transition: color 200ms var(--ease-premium);
	}

	.user-card:hover .nav-icon,
	.profile-header:hover .nav-icon {
		color: var(--color-foreground);
	}

	.user-card.active .nav-icon {
		color: var(--color-foreground);
	}

	/* Label - matches nav-item label style */
	.nav-label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		line-height: 1.4;
	}

	/* Icon button (collapsed mode) - matches nav-item collapsed style */
	.footer-icon-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		border-radius: 8px;
		background: transparent;
		color: var(--color-foreground-subtle);
		border: none;
		cursor: pointer;
		transition: all 200ms var(--ease-premium);
	}

	.footer-icon-btn:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
	}

	.footer-icon-btn.active {
		background: color-mix(in srgb, var(--color-foreground) 9%, transparent);
		color: var(--color-foreground);
	}

</style>
