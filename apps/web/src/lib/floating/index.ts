/**
 * Floating UI System
 *
 * Floating elements (popovers, menus, toolbars) placed with @floating-ui/dom.
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

// Core components
export { default as FloatingContent } from './core/FloatingContent.svelte';

// Primitives
export { default as Popover } from './primitives/Popover.svelte';
