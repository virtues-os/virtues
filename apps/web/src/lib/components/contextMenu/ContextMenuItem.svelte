<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';

	interface Props {
		item: ContextMenuItem;
		focused?: boolean;
		onHover?: () => void;
	}

	let { item, focused = false, onHover }: Props = $props();

	const isLoading = $derived(contextMenu.loadingItemId === item.id);

	function handleClick(e: MouseEvent) {
		e.stopPropagation();
		if (item.submenu) {
			contextMenu.openSubmenu(item.id);
		} else {
			contextMenu.executeAction(item);
		}
	}

	function handleMouseEnter() {
		onHover?.();
		if (item.submenu) {
			contextMenu.openSubmenu(item.id);
		}
	}
</script>

{#if item.dividerBefore}
	<div class="divider"></div>
{/if}

<button
	class="menu-item"
	class:focused
	class:disabled={item.disabled}
	class:checked={item.checked}
	class:destructive={item.variant === 'destructive'}
	class:has-submenu={!!item.submenu}
	class:loading={isLoading}
	onclick={handleClick}
	onmouseenter={handleMouseEnter}
	disabled={item.disabled || isLoading}
	role="menuitem"
	aria-disabled={item.disabled}
	aria-haspopup={item.submenu ? 'menu' : undefined}
	aria-expanded={item.submenu && contextMenu.openSubmenuId === item.id ? 'true' : undefined}
>
	{#if item.icon}
		<span class="item-icon">
			{#if isLoading}
				<Icon icon="ri:loader-4-line" width="16" class="spin" />
			{:else}
				<Icon icon={item.icon} width="16" />
			{/if}
		</span>
	{:else if isLoading}
		<span class="item-icon">
			<Icon icon="ri:loader-4-line" width="16" class="spin" />
		</span>
	{/if}

	<span class="item-label">{item.label}</span>

	{#if item.shortcut}
		<span class="item-shortcut">{item.shortcut}</span>
	{/if}

	{#if item.submenu}
		<span class="item-chevron">
			<Icon icon="ri:arrow-right-s-line" width="16" />
		</span>
	{/if}
</button>

{#if item.dividerAfter}
	<div class="divider"></div>
{/if}

<style>
	@reference "../../../app.css";

	.menu-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 6px 10px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground);
		font-size: 13px;
		text-align: left;
		cursor: pointer;
		transition: background-color 100ms ease;
	}

	.menu-item:hover:not(.disabled),
	.menu-item.focused:not(.disabled) {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}

	.menu-item.disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.menu-item.destructive {
		color: var(--color-error, #ef4444);
	}

	.menu-item.destructive:hover:not(.disabled),
	.menu-item.destructive.focused:not(.disabled) {
		background: color-mix(in srgb, var(--color-error, #ef4444) 12%, transparent);
	}

	.menu-item.loading {
		pointer-events: none;
	}

	.item-icon {
		flex-shrink: 0;
		width: 16px;
		height: 16px;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-foreground-muted);
	}

	.menu-item.destructive .item-icon {
		color: var(--color-error, #ef4444);
	}

	.menu-item.checked {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}

	.item-label {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.item-shortcut {
		flex-shrink: 0;
		font-size: 11px;
		color: var(--color-foreground-muted);
		opacity: 0.7;
	}

	.item-chevron {
		flex-shrink: 0;
		margin-left: auto;
		color: var(--color-foreground-muted);
	}

	.divider {
		height: 1px;
		background: var(--color-border);
		margin: 4px 8px;
	}

	/* Spin animation for loading state */
	:global(.spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}
</style>
