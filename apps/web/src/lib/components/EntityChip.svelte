<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { getEntityRoute, entityTypeIcon, getEntityTypeFromRoute } from "$lib/utils/entityRoutes";

	let { displayName, entityId, url, entityType } = $props<{
		displayName: string;
		entityId?: string;
		url?: string;
		entityType?: string;
	}>();

	// Get the navigation URL
	function getNavigationUrl(): string {
		if (url) return url;
		if (entityId) return getEntityRoute(entityId);
		return "#";
	}

	// Type drives the leading icon; derive from the url if not passed explicitly.
	const resolvedType = $derived(entityType ?? (url ? getEntityTypeFromRoute(url) : null));

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		windowShellStore.openTabFromRoute(getNavigationUrl(), {
			forceNew: true,
			preferEmptyPane: true,
		});
	}
</script>

<button class="entity-chip" onclick={handleClick} title="View {displayName}"
	><Icon icon={entityTypeIcon(resolvedType)} width="11" class="entity-chip-icon" />@{displayName}</button
>

<style>
	.entity-chip :global(.entity-chip-icon) {
		display: inline-block;
		vertical-align: -0.13em;
		margin-right: 1px;
		opacity: 0.85;
	}
</style>
