/**
 * Active Page Store (Backward Compatibility)
 *
 * This file re-exports from editAllowList.svelte.ts for backward compatibility.
 * New code should import from editAllowList.svelte.ts directly.
 *
 * @deprecated Use editAllowListStore from './editAllowList.svelte.ts' instead
 */

export {
	activePageStore,
	type EditableResourceContext,
	type EditableResourceType,
	type EditAllowListItem,
	editAllowListStore
} from './editAllowList.svelte';
