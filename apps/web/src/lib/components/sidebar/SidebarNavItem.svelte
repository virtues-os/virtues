<script lang="ts">
	import { page } from '$app/state';
	import 'iconify-icon';
	import type { SidebarNavItemData } from './types';

	interface Props {
		item: SidebarNavItemData;
		collapsed?: boolean;
		animationDelay?: number;
	}

	let { item, collapsed = false, animationDelay = 0 }: Props = $props();

	function isActive(href?: string, pagespace?: string): boolean {
		if (!href) return false;

		// For chat routes with conversationId query param
		if (pagespace) {
			const currentConversationId = page.url.searchParams.get('conversationId');
			if (currentConversationId === pagespace) {
				return true;
			}
		}

		if (page.url.pathname === href) {
			return true;
		}

		if (pagespace === '') {
			return page.url.pathname === '/';
		}

		if (pagespace) {
			return page.url.pathname.startsWith(`/${pagespace}`);
		}

		return false;
	}

	const active = $derived(isActive(item.href, item.pagespace));
</script>

{#if item.type === 'action'}
	<button
		onclick={item.onclick}
		class="nav-item"
		class:collapsed
		title={collapsed ? item.label : undefined}
		style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay + 400}ms"
	>
		{#if item.icon}
			<iconify-icon icon={item.icon} width="16" class="nav-icon"></iconify-icon>
		{/if}
		{#if !collapsed}
			<span class="nav-label">{item.label}</span>
		{/if}
	</button>
{:else}
	<a
		href={item.href}
		class="nav-item"
		class:active
		class:collapsed
		title={collapsed ? item.label : undefined}
		style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay + 400}ms"
	>
		{#if item.statusIcon}
			<iconify-icon
				icon={item.statusIcon}
				width="16"
				class="nav-icon status-icon"
			></iconify-icon>
		{:else if item.icon}
			<iconify-icon icon={item.icon} width="16" class="nav-icon"></iconify-icon>
		{/if}
		{#if !collapsed}
			<span class="nav-label">{item.label}</span>
		{/if}
	</a>
{/if}

<style>
	@reference "../../../app.css";

	/* Premium easing - heavy friction feel */
	:root {
		--ease-premium: cubic-bezier(0.2, 0.0, 0, 1.0);
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

	.nav-item {
		@apply flex items-center cursor-pointer;
		@apply rounded-lg; /* Pill-style rounded corners */
		gap: 10px;
		padding: 6px 10px; /* More vertical padding for breathing room */
		font-size: 13px;
		color: rgba(0, 0, 0, 0.55);
		/* Staggered load animation (initial mount) */
		animation: fadeSlideIn 200ms var(--ease-premium) backwards;
		/* Staggered expand transition (sidebar open) - uses --stagger-delay CSS var */
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--ease-premium) var(--stagger-delay, 400ms),
			transform 200ms var(--ease-premium) var(--stagger-delay, 400ms),
			background-color 200ms var(--ease-premium),
			color 200ms var(--ease-premium);
	}

	/* Dark mode inactive text */
	:global([data-theme="dark"]) .nav-item,
	:global([data-theme="night"]) .nav-item {
		color: rgba(255, 255, 255, 0.55);
	}

	/* Smooth hover with micro-interaction */
	.nav-item:hover {
		background: rgba(0, 0, 0, 0.05);
		color: rgba(0, 0, 0, 0.8);
		transform: translateX(2px); /* Subtle rightward shift */
	}

	/* Dark mode hover */
	:global([data-theme="dark"]) .nav-item:hover,
	:global([data-theme="night"]) .nav-item:hover {
		background: rgba(255, 255, 255, 0.08);
		color: rgba(255, 255, 255, 0.8);
	}

	/* Active/pressed state */
	.nav-item:active {
		transform: scale(0.98);
	}

	/* Active state - Zinc shadow style, NOT blue highlight */
	.nav-item.active {
		background: rgba(0, 0, 0, 0.07); /* Zinc-200/50 equivalent */
		color: rgba(0, 0, 0, 0.9); /* Near-black text */
		font-weight: 500;
	}

	/* Dark mode active */
	:global([data-theme="dark"]) .nav-item.active,
	:global([data-theme="night"]) .nav-item.active {
		background: rgba(255, 255, 255, 0.12);
		color: rgba(255, 255, 255, 0.95);
	}

	.nav-item.collapsed {
		@apply justify-center;
		padding: 0;
		width: 32px;
		height: 32px;
		margin: 0 auto;
		border-radius: 8px;
	}

	/* Icon opacity strategy: light by default, darken on hover/active */
	.nav-icon {
		@apply shrink-0;
		color: rgba(0, 0, 0, 0.4); /* Light gray default - reduces visual clutter */
		transition: all 200ms var(--ease-premium);
	}

	:global([data-theme="dark"]) .nav-icon,
	:global([data-theme="night"]) .nav-icon {
		color: rgba(255, 255, 255, 0.4);
	}

	.nav-item:hover .nav-icon {
		color: rgba(0, 0, 0, 0.7); /* Darken on hover */
	}

	:global([data-theme="dark"]) .nav-item:hover .nav-icon,
	:global([data-theme="night"]) .nav-item:hover .nav-icon {
		color: rgba(255, 255, 255, 0.7);
	}

	.nav-item.active .nav-icon {
		color: rgba(0, 0, 0, 0.75); /* Darkest on active */
	}

	:global([data-theme="dark"]) .nav-item.active .nav-icon,
	:global([data-theme="night"]) .nav-item.active .nav-icon {
		color: rgba(255, 255, 255, 0.8);
	}

	.status-icon {
		color: var(--success) !important;
	}

	.nav-label {
		@apply truncate;
		line-height: 1.4;
	}
</style>
