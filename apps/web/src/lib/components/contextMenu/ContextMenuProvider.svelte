<script lang="ts">
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import ContextMenuItem from './ContextMenuItem.svelte';
	import ContextMenuSubmenu from './ContextMenuSubmenu.svelte';
	import { onMount, onDestroy } from 'svelte';
	import { useFloating } from '$lib/floating';
	import type { Placement } from '@floating-ui/dom';

	let menuRef = $state<HTMLElement | null>(null);
	let menuRect = $state<DOMRect | null>(null);
	let itemRects = $state<Map<string, DOMRect>>(new Map());

	// Use the new floating hook for smart positioning
	const floating = useFloating(
		() => contextMenu.anchor,
		() => menuRef,
		() => null,
		{
			placement: contextMenu.placement as Placement,
			flip: true,
			shift: true,
			padding: 8,
			offset: 4
		}
	);

	// Update menu rect when visible (for submenu positioning)
	$effect(() => {
		if (contextMenu.visible && menuRef) {
			// Use requestAnimationFrame to wait for render
			requestAnimationFrame(() => {
				if (menuRef) {
					menuRect = menuRef.getBoundingClientRect();
				}
			});
		}
	});

	// Compute actual position to use - use floating position when anchor provided, fallback to store position
	const menuPosition = $derived.by(() => {
		if (contextMenu.anchor) {
			return floating.state;
		}
		return contextMenu.position;
	});

	// Find the rect for a submenu's parent item
	function getItemRect(itemId: string): DOMRect | null {
		return itemRects.get(itemId) ?? null;
	}

	function handleBackdropClick(e: MouseEvent) {
		// Only close if clicking the backdrop itself
		if (e.target === e.currentTarget) {
			contextMenu.hide();
		}
	}

	function handleBackdropContextMenu(e: MouseEvent) {
		e.preventDefault();
		contextMenu.hide();
	}

	function handleKeydown(e: KeyboardEvent) {
		if (!contextMenu.visible) return;

		switch (e.key) {
			case 'Escape':
				e.preventDefault();
				if (contextMenu.openSubmenuId) {
					contextMenu.closeSubmenu();
				} else {
					contextMenu.hide();
				}
				break;
			case 'ArrowDown':
				e.preventDefault();
				contextMenu.focusNext();
				break;
			case 'ArrowUp':
				e.preventDefault();
				contextMenu.focusPrevious();
				break;
			case 'ArrowRight':
				e.preventDefault();
				// Open submenu if focused item has one
				if (contextMenu.focusedIndex >= 0) {
					const item = contextMenu.items[contextMenu.focusedIndex];
					if (item?.submenu) {
						contextMenu.openSubmenu(item.id);
					}
				}
				break;
			case 'ArrowLeft':
				e.preventDefault();
				contextMenu.closeSubmenu();
				break;
			case 'Enter':
			case ' ':
				e.preventDefault();
				contextMenu.activateFocused();
				break;
		}
	}

	// Track item elements for submenu positioning
	function trackItemElement(itemId: string, element: HTMLElement | null) {
		if (element) {
			const rect = element.getBoundingClientRect();
			itemRects.set(itemId, rect);
			itemRects = itemRects; // Trigger reactivity
		}
	}

	// ---- Long-press → context menu (touch) -------------------------------
	// iOS WKWebView never fires `contextmenu` for touch, so every row action
	// gated behind right-click is unreachable on the phone. Synthesize one
	// after a still-press; it bubbles to the views' existing oncontextmenu
	// handlers, so this one hook covers Pages/Drive/Notebooks/etc. at once.
	const LONG_PRESS_MS = 450;
	const MOVE_TOLERANCE_PX = 10;
	let lpTimer: ReturnType<typeof setTimeout> | null = null;
	let lpTarget: EventTarget | null = null;
	let lpX = 0;
	let lpY = 0;

	function cancelLongPress() {
		if (lpTimer) {
			clearTimeout(lpTimer);
			lpTimer = null;
		}
		lpTarget = null;
	}

	function onPointerDown(e: PointerEvent) {
		if (e.pointerType !== 'touch' || !e.isPrimary || contextMenu.visible) return;
		// Don't fight iOS text selection / editor long-press behavior.
		const el = e.target as HTMLElement | null;
		if (el?.closest('input, textarea, [contenteditable="true"], .cm-editor')) return;
		lpTarget = e.target;
		lpX = e.clientX;
		lpY = e.clientY;
		lpTimer = setTimeout(() => {
			lpTimer = null;
			const target = lpTarget as HTMLElement | null;
			lpTarget = null;
			if (!target?.isConnected) return;
			const evt = new MouseEvent('contextmenu', {
				bubbles: true,
				cancelable: true,
				clientX: lpX,
				clientY: lpY
			});
			// preventDefault() from a handler (or the menu becoming visible)
			// means someone owned it — then eat the click that fires when the
			// finger lifts, or it would instantly close the menu via backdrop.
			const owned = !target.dispatchEvent(evt) || contextMenu.visible;
			if (owned) suppressNextClick();
		}, LONG_PRESS_MS);
	}

	function onPointerMove(e: PointerEvent) {
		if (!lpTimer) return;
		if (Math.hypot(e.clientX - lpX, e.clientY - lpY) > MOVE_TOLERANCE_PX) {
			cancelLongPress();
		}
	}

	function suppressNextClick() {
		const stop = (ce: MouseEvent) => {
			ce.preventDefault();
			ce.stopPropagation();
			cleanup();
		};
		const cleanup = () => window.removeEventListener('click', stop, true);
		window.addEventListener('click', stop, true);
		// The lift-click arrives within a frame or two; don't linger.
		setTimeout(cleanup, 700);
	}

	onMount(() => {
		window.addEventListener('keydown', handleKeydown);
		window.addEventListener('pointerdown', onPointerDown, true);
		window.addEventListener('pointermove', onPointerMove, { passive: true });
		window.addEventListener('pointerup', cancelLongPress, true);
		window.addEventListener('pointercancel', cancelLongPress, true);
		window.addEventListener('scroll', cancelLongPress, true);
	});

	onDestroy(() => {
		if (typeof window !== 'undefined') {
			window.removeEventListener('keydown', handleKeydown);
			window.removeEventListener('pointerdown', onPointerDown, true);
			window.removeEventListener('pointermove', onPointerMove);
			window.removeEventListener('pointerup', cancelLongPress, true);
			window.removeEventListener('pointercancel', cancelLongPress, true);
			window.removeEventListener('scroll', cancelLongPress, true);
			cancelLongPress();
		}
	});

	// Get items with submenus for rendering their submenus
	const itemsWithSubmenu = $derived(
		contextMenu.items.filter(item => item.submenu && contextMenu.openSubmenuId === item.id)
	);
</script>

{#if contextMenu.visible}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div
		class="context-menu-backdrop"
		onclick={handleBackdropClick}
		oncontextmenu={handleBackdropContextMenu}
	>
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<div
			bind:this={menuRef}
			class="context-menu"
			style="top: {menuPosition.y}px; left: {menuPosition.x}px"
			role="menu"
			aria-label="Context menu"
			onclick={(e) => e.stopPropagation()}
		>
			{#each contextMenu.items as item, index (item.id)}
				{@const hasSubmenu = !!item.submenu}
				<div
					class="item-wrapper"
					use:trackItem={{ itemId: item.id, hasSubmenu }}
				>
					<ContextMenuItem
						{item}
						focused={contextMenu.focusedIndex === index}
						onHover={() => {
							contextMenu.focusedIndex = index;
							// Update rect when hovering for submenu positioning
							const wrapper = document.querySelector(`[data-item-id="${item.id}"]`);
							if (wrapper) {
								itemRects.set(item.id, wrapper.getBoundingClientRect());
								itemRects = itemRects;
							}
						}}
					/>
				</div>
			{/each}
		</div>

		<!-- Render submenus -->
		{#each itemsWithSubmenu as item (item.id)}
			<ContextMenuSubmenu {item} parentRect={getItemRect(item.id)} />
		{/each}
	</div>
{/if}

<script module lang="ts">
	// Svelte action to track item elements
	function trackItem(node: HTMLElement, params: { itemId: string; hasSubmenu: boolean }) {
		node.setAttribute('data-item-id', params.itemId);
		return {
			update(newParams: { itemId: string; hasSubmenu: boolean }) {
				node.setAttribute('data-item-id', newParams.itemId);
			}
		};
	}
</script>

<style>
	@reference "../../../app.css";

	.context-menu-backdrop {
		position: fixed;
		inset: 0;
		z-index: var(--z-context-menu);
		background: transparent;
	}

	.context-menu {
		position: fixed;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.16);
		padding: 4px;
		min-width: 180px;
		max-width: 280px;
		/* Keep clear of the Dynamic Island / home indicator on the phone. */
		max-height: calc(
			100dvh - max(16px, env(safe-area-inset-top)) - max(16px, env(safe-area-inset-bottom))
		);
		overflow-y: auto;
		animation: menu-fade-in 100ms ease-out;
	}

	@keyframes menu-fade-in {
		from {
			opacity: 0;
			transform: scale(0.95);
		}
		to {
			opacity: 1;
			transform: scale(1);
		}
	}

	.item-wrapper {
		/* Wrapper for tracking item positions */
	}
</style>
