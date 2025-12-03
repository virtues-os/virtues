<script lang="ts">
	import 'iconify-icon';
	import { page } from '$app/state';
	import SidebarTooltip from './SidebarTooltip.svelte';

	interface Props {
		collapsed?: boolean;
	}

	let { collapsed = false }: Props = $props();

	// Check if settings is the active route
	const isSettingsActive = $derived(
		page.url.pathname.startsWith('/profile')
	);
</script>

<div class="footer" class:collapsed style="--stagger-delay: 500ms">
	{#if collapsed}
		<!-- Collapsed: just show avatar and settings icon -->
		<SidebarTooltip content="Settings">
			<a href="/profile/account" class="footer-icon-btn" class:active={isSettingsActive}>
				<iconify-icon icon="ri:settings-4-line" width="18"></iconify-icon>
			</a>
		</SidebarTooltip>
	{:else}
		<!-- Expanded: show settings link - clean, no avatar box -->
		<a href="/profile/account" class="user-card" class:active={isSettingsActive}>
			<div class="avatar">
				<iconify-icon icon="ri:settings-4-line" width="16"></iconify-icon>
			</div>
			<div class="user-info">
				<span class="user-name">Settings</span>
			</div>
		</a>
	{/if}
</div>

<style>
	@reference "../../../app.css";

	/* Premium easing - heavy friction feel */
	:root {
		--ease-premium: cubic-bezier(0.2, 0.0, 0, 1.0);
	}

	.footer {
		@apply flex flex-col gap-1 py-3 mt-auto;
		padding-left: 8px;
		padding-right: 8px;
		/* Staggered expand transition - uses --stagger-delay CSS var */
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--ease-premium) var(--stagger-delay, 500ms),
			transform 200ms var(--ease-premium) var(--stagger-delay, 500ms);
	}

	.footer.collapsed {
		@apply items-center;
		padding-left: 4px;
		padding-right: 4px;
	}

	/* User card - matches nav-item pill style exactly */
	.user-card {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 10px;
		border-radius: 8px;
		text-decoration: none;
		background: transparent; /* Explicitly no background */
		color: rgba(0, 0, 0, 0.55);
		/* Smooth micro-interaction with premium easing */
		transition: all 200ms var(--ease-premium);
	}

	:global([data-theme="dark"]) .user-card,
	:global([data-theme="night"]) .user-card {
		color: rgba(255, 255, 255, 0.55);
	}

	.user-card:hover {
		background: rgba(0, 0, 0, 0.05);
		color: rgba(0, 0, 0, 0.8);
	}

	:global([data-theme="dark"]) .user-card:hover,
	:global([data-theme="night"]) .user-card:hover {
		background: rgba(255, 255, 255, 0.08);
		color: rgba(255, 255, 255, 0.8);
	}

	/* Active state - zinc shadow style matching nav items */
	.user-card.active {
		background: rgba(0, 0, 0, 0.07);
		color: rgba(0, 0, 0, 0.9);
	}

	:global([data-theme="dark"]) .user-card.active,
	:global([data-theme="night"]) .user-card.active {
		background: rgba(255, 255, 255, 0.12);
		color: rgba(255, 255, 255, 0.95);
	}

	/* Avatar icon container - fully transparent, no sticker effect */
	.avatar {
		width: 16px;
		height: 16px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: transparent; /* No white background */
		color: rgba(0, 0, 0, 0.4); /* Light icon by default */
		transition: all 200ms var(--ease-premium);
	}

	:global([data-theme="dark"]) .avatar,
	:global([data-theme="night"]) .avatar {
		color: rgba(255, 255, 255, 0.4);
	}

	.user-card:hover .avatar {
		color: rgba(0, 0, 0, 0.7);
	}

	:global([data-theme="dark"]) .user-card:hover .avatar,
	:global([data-theme="night"]) .user-card:hover .avatar {
		color: rgba(255, 255, 255, 0.7);
	}

	.user-card.active .avatar {
		color: rgba(0, 0, 0, 0.75);
	}

	:global([data-theme="dark"]) .user-card.active .avatar,
	:global([data-theme="night"]) .user-card.active .avatar {
		color: rgba(255, 255, 255, 0.8);
	}

	.user-info {
		flex: 1;
		min-width: 0;
	}

	.user-name {
		font-size: 13px;
		font-weight: 500;
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
		color: rgba(0, 0, 0, 0.4);
		transition: all 200ms var(--ease-premium);
	}

	:global([data-theme="dark"]) .footer-icon-btn,
	:global([data-theme="night"]) .footer-icon-btn {
		color: rgba(255, 255, 255, 0.4);
	}

	.footer-icon-btn:hover {
		background: rgba(0, 0, 0, 0.05);
		color: rgba(0, 0, 0, 0.7);
	}

	:global([data-theme="dark"]) .footer-icon-btn:hover,
	:global([data-theme="night"]) .footer-icon-btn:hover {
		background: rgba(255, 255, 255, 0.08);
		color: rgba(255, 255, 255, 0.7);
	}

	.footer-icon-btn.active {
		background: rgba(0, 0, 0, 0.07);
		color: rgba(0, 0, 0, 0.75);
	}

	:global([data-theme="dark"]) .footer-icon-btn.active,
	:global([data-theme="night"]) .footer-icon-btn.active {
		background: rgba(255, 255, 255, 0.12);
		color: rgba(255, 255, 255, 0.8);
	}
</style>
