<script lang="ts">
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { getEntityRoute } from "$lib/utils/entityRoutes";

	let { displayName, entityId, url } = $props<{
		displayName: string;
		entityId?: string;
		url?: string;
	}>();

	// Get the navigation URL
	function getNavigationUrl(): string {
		if (url) return url;
		if (entityId) return getEntityRoute(entityId);
		return "#";
	}

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		windowShellStore.openTabFromRoute(getNavigationUrl(), {
			forceNew: true,
			preferEmptyPane: true,
		});
	}
</script>

<button class="entity-chip" onclick={handleClick} title="View {displayName}">@{displayName}</button>
