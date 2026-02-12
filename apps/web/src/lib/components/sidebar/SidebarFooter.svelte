<script lang="ts">
	import { goto } from "$app/navigation";
	import Icon from "$lib/components/Icon.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import SidebarNavItem from "./SidebarNavItem.svelte";
	import SidebarTooltip from "./SidebarTooltip.svelte";
	import type { SidebarNavItemData } from "./types";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let {
		collapsed = false,
		animationDelay = 0,
	}: Props = $props();

	// Persist expand/collapse across reloads
	let isSettingsExpanded = $state(
		typeof localStorage !== 'undefined' && localStorage.getItem('sidebar-settings-expanded') === 'true'
	);
	$effect(() => {
		localStorage.setItem('sidebar-settings-expanded', String(isSettingsExpanded));
	});

	// Tab-system-based active detection (works in split view)
	const isSettingsActive = $derived.by(() => {
		const _activeTabId = spaceStore.activeTabId;
		const _splitEnabled = spaceStore.isSplit;
		const activeTabs = spaceStore.getActiveTabsForSidebar();
		return activeTabs.some(t => t.route.startsWith('/virtues/'));
	});

	function toggleSettings() {
		isSettingsExpanded = !isSettingsExpanded;
	}

	async function handleLogout() {
		try {
			const response = await fetch("/auth/signout", { method: "POST" });
			if (response.ok) {
				spaceStore.closeAllTabs();
				await goto("/login");
			} else {
				console.error("[Logout] Failed to sign out:", response.status);
				await goto("/login");
			}
		} catch (error) {
			console.error("[Logout] Error:", error);
			await goto("/login");
		}
	}

	const settingsItems: SidebarNavItemData[] = [
		{ id: 'settings-profile', type: 'link', label: 'Profile', icon: 'ri:user-3-line', href: '/virtues/account' },
		{ id: 'settings-assistant', type: 'link', label: 'Assistant', icon: 'ri:robot-line', href: '/virtues/assistant' },
		{ id: 'settings-billing', type: 'link', label: 'Billing', icon: 'ri:bank-card-line', href: '/virtues/billing' },
		{ id: 'settings-system', type: 'link', label: 'System', icon: 'ri:computer-line', href: '/virtues/system' },
		{ id: 'settings-feedback', type: 'link', label: 'Feedback', icon: 'ri:feedback-line', href: '/virtues/feedback' },
	];

	const signOutItem: SidebarNavItemData = {
		id: 'settings-signout',
		type: 'action',
		label: 'Sign Out',
		icon: 'ri:logout-box-r-line',
		onclick: handleLogout,
	};
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
				class:active={isSettingsActive}
				aria-label="Settings"
				title="Settings"
			>
				<Icon icon="ri:settings-4-line" width="18" />
			</button>
		</SidebarTooltip>
	{:else}
		<!-- Settings folder header — icon↔chevron toggle (matches UnifiedFolder) -->
		<button
			onclick={toggleSettings}
			class="sidebar-interactive"
			class:active={isSettingsActive}
		>
			<span class="folder-toggle" class:expanded={isSettingsExpanded}>
				<span class="folder-toggle-icon">
					<Icon icon="ri:settings-4-line" width="16" class="sidebar-icon" />
				</span>
				<svg
					class="folder-toggle-chevron"
					width="12"
					height="12"
					viewBox="0 0 16 16"
					fill="none"
				>
					<path
						d="M6 4L10 8L6 12"
						stroke="currentColor"
						stroke-width="1.5"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				</svg>
			</span>
			<span class="sidebar-label">Settings</span>
		</button>

		<!-- Settings children — CSS grid expand/collapse (matches UnifiedFolder) -->
		<div class="sidebar-expandable-content" class:expanded={isSettingsExpanded}>
			<div class="sidebar-expandable-overflow">
				<div class="sidebar-expandable-inner children-inner">
					{#each settingsItems as item (item.id)}
						<SidebarNavItem {item} indent={1} {collapsed} isSystemItem={true} />
					{/each}
					<div class="logout-wrapper">
						<SidebarNavItem item={signOutItem} indent={1} {collapsed} isSystemItem={true} />
					</div>
				</div>
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

	/* ------- Icon ↔ Chevron slide toggle (mirrors UnifiedFolder) ------- */
	.folder-toggle {
		position: relative;
		width: 16px;
		height: 16px;
		flex-shrink: 0;
		overflow: hidden;
		cursor: pointer;
	}

	.folder-toggle-icon,
	.folder-toggle-chevron {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		transition:
			opacity 120ms ease,
			transform 160ms ease;
	}

	/* Default: icon visible, chevron hidden below */
	.folder-toggle-icon {
		opacity: 1;
		transform: translateY(0);
	}

	.folder-toggle-chevron {
		opacity: 0;
		transform: translateY(6px);
		color: var(--color-foreground-subtle);
		margin: auto;
	}

	/* Hover: icon slides up, chevron slides up into place */
	.sidebar-interactive:hover .folder-toggle-icon {
		opacity: 0;
		transform: translateY(-6px);
	}

	.sidebar-interactive:hover .folder-toggle-chevron {
		opacity: 1;
		transform: translateY(0);
	}

	/* Expanded: always show chevron rotated 90°, hide icon */
	.folder-toggle.expanded .folder-toggle-icon {
		opacity: 0;
		transform: translateY(-6px);
	}

	.folder-toggle.expanded .folder-toggle-chevron {
		opacity: 1;
		transform: translateY(0) rotate(90deg);
	}

	/* Expanded + hover: keep rotated */
	.sidebar-interactive:hover .folder-toggle.expanded .folder-toggle-chevron {
		transform: translateY(0) rotate(90deg);
	}

	/* ------- CSS grid expand/collapse (mirrors UnifiedFolder) ------- */
	.sidebar-expandable-content {
		display: grid;
		grid-template-rows: 0fr;
		transition: grid-template-rows 150ms ease;
	}

	.sidebar-expandable-content.expanded {
		grid-template-rows: 1fr;
	}

	.sidebar-expandable-overflow {
		overflow: hidden;
		padding-top: 4px;
	}

	/* Children indent - uses shared sidebar variable */
	.children-inner {
		padding-left: var(--sidebar-indent-width);
	}

	/* Sign Out red hover state */
	.logout-wrapper :global(.sidebar-interactive):hover {
		background: color-mix(in srgb, var(--color-error) 15%, transparent);
		color: var(--color-error);
	}

	.logout-wrapper :global(.sidebar-interactive):hover :global(.sidebar-icon) {
		color: var(--color-error);
	}
</style>
