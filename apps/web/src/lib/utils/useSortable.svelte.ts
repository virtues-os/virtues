/**
 * SortableJS integration for Svelte 5
 *
 * Provides a clean hook pattern using $effect for lifecycle management.
 * Replaces the complex svelte-dnd-action consider/finalize model with
 * SortableJS's simpler onEnd callback pattern.
 */

import Sortable from 'sortablejs';
import type { Options, SortableEvent, MoveEvent } from 'sortablejs';

/**
 * Svelte 5 hook for SortableJS integration.
 *
 * Uses $effect for lifecycle management - the sortable instance is created
 * when the element becomes available and destroyed on cleanup.
 *
 * @param getter - Function that returns the target element (supports reactive binding)
 * @param options - Function that returns SortableJS options (supports reactive options)
 *
 * @example
 * ```svelte
 * <script>
 *   let listEl = $state<HTMLElement | null>(null);
 *
 *   useSortable(
 *     () => listEl,
 *     () => ({
 *       animation: 150,
 *       onEnd(evt) {
 *         items = reorder(items, evt);
 *       }
 *     })
 *   );
 * </script>
 *
 * <ul bind:this={listEl}>
 *   {#each items as item (item.id)}
 *     <li>{item.name}</li>
 *   {/each}
 * </ul>
 * ```
 */
export function useSortable(
	getter: () => HTMLElement | null,
	options: () => Options
): void {
	$effect(() => {
		const el = getter();
		if (!el) return;

		const sortable = Sortable.create(el, options());

		return () => sortable.destroy();
	});
}

/**
 * Reorder an array after a drag operation within the same zone.
 *
 * Creates a new array with the item moved from oldIndex to newIndex.
 * Returns the original array if indices are invalid or unchanged.
 *
 * @param array - The original array
 * @param evt - SortableJS event containing oldIndex and newIndex
 * @returns New array with reordered items
 *
 * @example
 * ```typescript
 * onEnd(evt) {
 *   items = reorder(items, evt);
 *   await persistOrder(items);
 * }
 * ```
 */
export function reorder<T>(array: T[], evt: SortableEvent): T[] {
	const { oldIndex, newIndex } = evt;

	if (oldIndex === undefined || newIndex === undefined) return array;
	if (oldIndex === newIndex) return array;

	const result = [...array];
	const [item] = result.splice(oldIndex, 1);
	result.splice(newIndex, 0, item);
	return result;
}

/**
 * Move an item between two arrays (for cross-zone drops).
 *
 * Creates new arrays with the item removed from source and inserted into destination.
 *
 * @param from - Source array
 * @param to - Destination array
 * @param fromIndex - Index in source array
 * @param toIndex - Index in destination array
 * @returns Object with new from and to arrays
 *
 * @example
 * ```typescript
 * onAdd(evt) {
 *   const { from, to } = moveItem(sourceItems, destItems, evt.oldIndex, evt.newIndex);
 *   sourceItems = from;
 *   destItems = to;
 * }
 * ```
 */
export function moveItem<T>(
	from: T[],
	to: T[],
	fromIndex: number,
	toIndex: number
): { from: T[]; to: T[] } {
	const fromCopy = [...from];
	const toCopy = [...to];
	const [item] = fromCopy.splice(fromIndex, 1);
	toCopy.splice(toIndex, 0, item);
	return { from: fromCopy, to: toCopy };
}

/**
 * Copy an item from one array to another (for smart view drag-out).
 *
 * Creates a new destination array with the copied item inserted.
 * Source array is unchanged.
 *
 * @param from - Source array (unchanged)
 * @param to - Destination array
 * @param fromIndex - Index in source array
 * @param toIndex - Index in destination array
 * @returns New destination array with copied item
 */
export function copyItem<T>(from: T[], to: T[], fromIndex: number, toIndex: number): T[] {
	const toCopy = [...to];
	const item = from[fromIndex];
	if (item !== undefined) {
		toCopy.splice(toIndex, 0, item);
	}
	return toCopy;
}

/**
 * Type guard to check if an event is from a different zone (cross-zone drop).
 */
export function isCrossZoneDrop(evt: SortableEvent): boolean {
	return evt.from !== evt.to;
}

/**
 * Get the data-* attribute from a dragged element.
 * Useful for identifying folder IDs, item types, etc.
 */
export function getDataAttribute(el: HTMLElement, attr: string): string | null {
	return el.getAttribute(`data-${attr}`);
}

// Re-export types for convenience
export type { SortableEvent, MoveEvent, Options as SortableOptions };
