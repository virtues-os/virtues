<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { onMount, onDestroy } from 'svelte';

	interface Props {
		item: ContextMenuItem;
		parentRect: DOMRect | null;
	}

	let { item, parentRect }: Props = $props();

	let submenuRef = $state<HTMLElement | null>(null);
	let position = $state({ x: 0, y: 0 });
	let openDirection = $state<'right' | 'left'>('right');

	// Calculate submenu position
	$effect(() => {
		if (!parentRect || !submenuRef) return;

		const submenuRect = submenuRef.getBoundingClientRect();
		const viewportWidth = window.innerWidth;
		const viewportHeight = window.innerHeight;
		const padding = 8;

		let x: number;
		let y: number;

		// Try to position to the right first
		if (parentRect.right + submenuRect.width + padding <= viewportWidth) {
			x = parentRect.right - 4; // Slight overlap
			openDirection = 'right';
		} else {
			// Position to the left
			x = parentRect.left - submenuRect.width + 4;
			openDirection = 'left';
		}

		// Vertical position - align with the parent item
		y = parentRect.top;

		// Adjust if overflowing bottom
		if (y + submenuRect.height + padding > viewportHeight) {
			y = viewportHeight - submenuRect.height - padding;
		}

		// Ensure minimum y position
		if (y < padding) {
			y = padding;
		}

		position = { x, y };
	});

	function handleItemClick(subItem: ContextMenuItem) {
		if (subItem.disabled || subItem.submenu) return;
		contextMenu.executeAction(subItem);
	}

	function handleMouseLeave() {
		// Small delay before closing to allow moving to submenu
		setTimeout(() => {
			if (contextMenu.openSubmenuId === item.id) {
				// Check if mouse is over the submenu
				// If not, close it
			}
		}, 100);
	}
</script>

<div
	bind:this={submenuRef}
	class="submenu"
	class:open-left={openDirection === 'left'}
	style="top: {position.y}px; left: {position.x}px"
	role="menu"
	aria-label={item.label}
	onmouseleave={handleMouseLeave}
>
	{#each item.submenu ?? [] as subItem (subItem.id)}
		{#if subItem.dividerBefore}
			<div class="divider"></div>
		{/if}

		<button
			class="menu-item"
			class:disabled={subItem.disabled}
			class:destructive={subItem.variant === 'destructive'}
			onclick={() => handleItemClick(subItem)}
			disabled={subItem.disabled}
			role="menuitem"
			aria-disabled={subItem.disabled}
		>
			{#if subItem.icon}
				<span class="item-icon">
					<Icon icon={subItem.icon} width="16" />
				</span>
			{/if}

			{#if subItem.checked !== undefined}
				<span class="item-check">
					{#if subItem.checked}
						<Icon icon="ri:check-line" width="14" />
					{/if}
				</span>
			{/if}

			<span class="item-label">{subItem.label}</span>

			{#if subItem.shortcut}
				<span class="item-shortcut">{subItem.shortcut}</span>
			{/if}
		</button>

		{#if subItem.dividerAfter}
			<div class="divider"></div>
		{/if}
	{/each}
</div>

<style>
	@reference "../../../app.css";

	.submenu {
		position: fixed;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.16);
		padding: 4px;
		min-width: 160px;
		max-width: 280px;
		max-height: calc(100vh - 32px);
		overflow-y: auto;
		z-index: 10001;
		animation: submenu-fade-in 100ms ease-out;
	}

	.submenu.open-left {
		animation: submenu-fade-in-left 100ms ease-out;
	}

	@keyframes submenu-fade-in {
		from {
			opacity: 0;
			transform: translateX(-4px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	@keyframes submenu-fade-in-left {
		from {
			opacity: 0;
			transform: translateX(4px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

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

	.menu-item:hover:not(.disabled) {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}

	.menu-item.disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.menu-item.destructive {
		color: var(--color-error, #ef4444);
	}

	.menu-item.destructive:hover:not(.disabled) {
		background: color-mix(in srgb, var(--color-error, #ef4444) 12%, transparent);
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

	.item-check {
		flex-shrink: 0;
		width: 14px;
		height: 14px;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-foreground);
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

	.divider {
		height: 1px;
		background: var(--color-border);
		margin: 4px 8px;
	}
</style>
