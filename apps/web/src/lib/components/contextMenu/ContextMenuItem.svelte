<script lang="ts">
	import MenuItem from '$lib/components/MenuItem.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';

	/**
	 * The context menu's row: an adapter, not a row.
	 *
	 * The markup and CSS that used to live here IS the app's menu row, and it
	 * was unreachable from anywhere else because this component takes a
	 * `ContextMenuItem` off the store and calls the store on click. Six other
	 * menus re-rolled it under six names as a result. The row moved to
	 * `MenuItem.svelte`; what stays here is the only part that was ever
	 * specific to the context menu — the store, the submenu, the dividers.
	 *
	 * `iconColor` does not survive the move. It was a per-item inline `color:`
	 * on the glyph — a literal in the pane, which design-grammar.md §5 will not
	 * have. Nothing sets it: `ContextMenuItem.iconColor` is declared on the
	 * store's type and read by `ContextMenuSubmenu` (where it paints a color
	 * swatch, and still does), but no call site in the tree assigns it on a
	 * TOP-LEVEL item, which is all this component renders. If that changes, the
	 * answer is `MenuItem`'s `leading` snippet — a swatch is a leading element,
	 * not a tinted icon — or `destructive`, which is a state rather than a
	 * paint.
	 */
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
		item.onMouseEnter?.();
		if (item.submenu) {
			contextMenu.openSubmenu(item.id);
		}
	}

	function handleMouseLeave() {
		item.onMouseLeave?.();
		if (item.submenu && contextMenu.openSubmenuId === item.id) {
			contextMenu.scheduleSubmenuClose();
		}
	}
</script>

{#if item.dividerBefore}
	<div class="divider"></div>
{/if}

<MenuItem
	icon={item.icon}
	label={item.label}
	description={item.description}
	shortcut={item.shortcut}
	checked={item.checked}
	destructive={item.variant === 'destructive'}
	disabled={item.disabled}
	loading={isLoading}
	submenu={!!item.submenu}
	expanded={item.submenu ? contextMenu.openSubmenuId === item.id : undefined}
	{focused}
	onclick={handleClick}
	onmouseenter={handleMouseEnter}
	onmouseleave={handleMouseLeave}
/>

{#if item.dividerAfter}
	<div class="divider"></div>
{/if}

<style>
	.divider {
		height: 1px;
		background: var(--color-border);
		margin: 4px 8px;
	}
</style>
