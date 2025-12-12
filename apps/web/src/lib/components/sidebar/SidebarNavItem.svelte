<script lang="ts">
	import { page } from "$app/state";
	import "iconify-icon";
	import type { SidebarNavItemData } from "./types";

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
			const currentConversationId =
				page.url.searchParams.get("conversationId");
			if (currentConversationId === pagespace) {
				return true;
			}
		}

		if (page.url.pathname === href) {
			return true;
		}

		if (pagespace === "") {
			return page.url.pathname === "/";
		}

		if (pagespace) {
			return page.url.pathname.startsWith(`/${pagespace}`);
		}

		return false;
	}

	const active = $derived(isActive(item.href, item.pagespace));
</script>

{#if item.type === "action"}
	<button
		onclick={item.onclick}
		class="nav-item"
		class:collapsed
		title={collapsed ? item.label : undefined}
		style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay +
			400}ms"
	>
		{#if item.icon}
			<iconify-icon icon={item.icon} width="16" class="nav-icon"
			></iconify-icon>
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
		style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay +
			400}ms"
	>
		{#if item.statusIcon}
			<iconify-icon
				icon={item.statusIcon}
				width="16"
				class="nav-icon status-icon"
			></iconify-icon>
		{:else if item.icon}
			<iconify-icon icon={item.icon} width="16" class="nav-icon"
			></iconify-icon>
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

	.nav-item {
		@apply flex items-center cursor-pointer;
		@apply rounded-lg; /* Pill-style rounded corners */
		gap: 10px;
		padding: 6px 10px; /* More vertical padding for breathing room */
		font-size: 13px;
		color: var(--color-foreground-muted);
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

	/* Smooth hover with micro-interaction */
	.nav-item:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
		transform: translateX(2px); /* Subtle rightward shift */
	}

	/* Active/pressed state */
	.nav-item:active {
		transform: scale(0.98);
	}

	/* Active state - Zinc shadow style, NOT blue highlight */
	.nav-item.active {
		background: color-mix(in srgb, var(--color-foreground) 9%, transparent);
		color: var(--color-foreground);
		font-weight: 500;
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
		color: var(--color-foreground-subtle);
		transition: all 200ms var(--ease-premium);
	}

	.nav-item:hover .nav-icon {
		color: var(--color-foreground);
	}

	.nav-item.active .nav-icon {
		color: var(--color-foreground);
	}

	.status-icon {
		color: var(--success) !important;
	}

	.nav-label {
		@apply truncate;
		line-height: 1.4;
	}
</style>
