/**
 * Floating UI System
 *
 * A unified system for floating elements (tooltips, popovers, dropdowns, etc.)
 * built on @floating-ui/dom.
 */

// Core types
export type {
	Anchor,
	FloatingContext,
	FloatingOptions,
	FloatingState,
	Placement,
	Strategy,
	VirtualAnchor
} from './core/types';

export { isVirtualAnchor } from './core/types';

// Hooks
export { useFloating } from './hooks/useFloating.svelte';
export { useClickOutside } from './hooks/useClickOutside.svelte';
export { useEscapeKey } from './hooks/useEscapeKey.svelte';
export { useKeyboardNav, type KeyboardNavOptions } from './hooks/useKeyboardNav.svelte';

// Core components
export { default as FloatingContent } from './core/FloatingContent.svelte';

// Primitives
export { default as Tooltip } from './primitives/Tooltip.svelte';
export { default as Popover } from './primitives/Popover.svelte';
export { default as Dropdown } from './primitives/Dropdown.svelte';

// Composites
export { default as Select } from './composites/Select.svelte';
