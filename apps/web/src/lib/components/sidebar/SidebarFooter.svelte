<script lang="ts">
	import "iconify-icon";
	import { page } from "$app/state";
	import { goto } from "$app/navigation";
	import { windowTabs } from "$lib/stores/windowTabs.svelte";
	import SidebarTooltip from "./SidebarTooltip.svelte";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
		onOpenBookmarks?: () => void;
	}

	let { collapsed = false, animationDelay = 0, onOpenBookmarks }: Props = $props();

	// Check if settings is the active route
	const isSettingsActive = $derived(
		page.url.pathname.startsWith("/profile") ||
		windowTabs.activeTab?.type === 'profile'
	);

	function handleSettingsClick(e: MouseEvent) {
		e.preventDefault();
		windowTabs.openTabFromRoute('/profile/account', { label: 'Settings', preferEmptyPane: true });
	}

	async function handleLogout() {
		try {
			const response = await fetch('/auth/signout', { method: 'POST' });
			if (response.ok) {
				// Clear any client-side state
				windowTabs.closeAllTabs();
				// Redirect to login
				await goto('/login');
			} else {
				console.error('[Logout] Failed to sign out:', response.status);
				// Even if signout fails, redirect to login to clear session
				await goto('/login');
			}
		} catch (error) {
			console.error('[Logout] Error:', error);
			// On network error, still redirect to login
			await goto('/login');
		}
	}
</script>

<div
	class="footer"
	class:collapsed
	style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
>
	{#if collapsed}
		<!-- Collapsed: just show icons -->
		<SidebarTooltip content="Bookmarks (Cmd+B)">
			<button
				onclick={onOpenBookmarks}
				class="footer-icon-btn"
				aria-label="Bookmarks"
				title="Bookmarks (Cmd+B)"
			>
				<iconify-icon icon="ri:bookmark-line" width="18"></iconify-icon>
			</button>
		</SidebarTooltip>
		<SidebarTooltip content="Settings">
			<button
				onclick={handleSettingsClick}
				class="footer-icon-btn"
				class:active={isSettingsActive}
				aria-label="Settings"
				title="Settings"
			>
				<iconify-icon icon="ri:settings-4-line" width="18"></iconify-icon>
			</button>
		</SidebarTooltip>
		<SidebarTooltip content="Sign Out">
			<button
				onclick={handleLogout}
				class="footer-icon-btn logout-btn"
				aria-label="Sign Out"
				title="Sign Out"
			>
				<iconify-icon icon="ri:logout-box-r-line" width="18"></iconify-icon>
			</button>
		</SidebarTooltip>
	{:else}
		<!-- Expanded: show bookmarks and settings links -->
		<button
			onclick={onOpenBookmarks}
			class="user-card"
		>
			<iconify-icon icon="ri:bookmark-line" width="16" class="nav-icon"></iconify-icon>
			<span class="nav-label">Bookmarks</span>
			<kbd class="shortcut-hint">&#8984;B</kbd>
		</button>
		<button
			onclick={handleSettingsClick}
			class="user-card"
			class:active={isSettingsActive}
		>
			<iconify-icon icon="ri:settings-4-line" width="16" class="nav-icon"></iconify-icon>
			<span class="nav-label">Settings</span>
		</button>
		<button
			onclick={handleLogout}
			class="user-card logout-btn"
		>
			<iconify-icon icon="ri:logout-box-r-line" width="16" class="nav-icon"></iconify-icon>
			<span class="nav-label">Sign Out</span>
		</button>
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

	.user-card:hover .nav-icon {
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

	/* Keyboard shortcut hint */
	.shortcut-hint {
		margin-left: auto;
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 10px;
		padding: 2px 5px;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		border-radius: 4px;
		color: var(--color-foreground-subtle);
	}
</style>
