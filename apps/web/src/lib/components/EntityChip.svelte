<script lang="ts">
	import { spaceStore } from "$lib/stores/space.svelte";
	import Icon from "$lib/components/Icon.svelte";

	let { displayName, entityId, url } = $props<{
		displayName: string;
		entityId?: string;
		url?: string;
	}>();

	// Get icon based on URL pattern (new approach) or entity ID prefix (legacy)
	function getIcon(): string {
		if (url) {
			if (url.startsWith("/person/")) return "ri:user-line";
			if (url.startsWith("/place/")) return "ri:map-pin-line";
			if (url.startsWith("/org/")) return "ri:building-line";
			if (url.startsWith("/thing/")) return "ri:box-3-line";
			if (url.startsWith("/page/")) return "ri:file-text-line";
			if (url.startsWith("/day/")) return "ri:calendar-line";
			if (url.startsWith("/year/")) return "ri:calendar-2-line";
			if (url.startsWith("/source/")) return "ri:database-2-line";
			if (url.startsWith("/chat/")) return "ri:chat-3-line";
			if (url.startsWith("/drive/")) return "ri:file-line";
		}
		if (entityId) {
			if (entityId.startsWith("person_")) return "ri:user-line";
			if (entityId.startsWith("place_")) return "ri:map-pin-line";
			if (entityId.startsWith("org_")) return "ri:building-line";
			if (entityId.startsWith("file_")) return "ri:file-line";
			if (entityId.startsWith("page_")) return "ri:file-text-line";
			if (entityId.startsWith("thing_")) return "ri:box-3-line";
		}
		return "ri:links-line";
	}

	// Compute route from entity ID (legacy support)
	function getEntityRoute(id: string): string {
		if (id.startsWith("person_")) return `/person/${id}`;
		if (id.startsWith("place_")) return `/place/${id}`;
		if (id.startsWith("org_")) return `/org/${id}`;
		if (id.startsWith("thing_")) return `/thing/${id}`;
		if (id.startsWith("day_")) return `/day/${id}`;
		if (id.startsWith("year_")) return `/year/${id}`;
		if (id.startsWith("file_")) return `/drive/${id}`;
		if (id.startsWith("page_")) return `/page/${id}`;
		if (id.startsWith("chat_")) return `/chat/${id}`;
		if (id.startsWith("source_")) return `/source/${id}`;
		return `/person/${id}`; // fallback
	}

	// Get the navigation URL - prefer direct url, fall back to computed from entityId
	function getNavigationUrl(): string {
		if (url) return url;
		if (entityId) return getEntityRoute(entityId);
		return "#";
	}

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		spaceStore.openTabFromRoute(getNavigationUrl(), {
			forceNew: true,
			preferEmptyPane: true,
		});
	}
</script>

<button class="entity-chip" onclick={handleClick} title="View {displayName}">
	<span class="entity-icon">
		<Icon icon={getIcon()} width="14"/>
	</span>
	<span class="entity-text">{displayName}</span>
</button>

<style>
	.entity-chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 1px 8px 1px 6px;
		border-radius: 4px;
		background-color: color-mix(in srgb, var(--color-primary) 10%, transparent);
		color: var(--color-primary);
		text-decoration: none;
		cursor: pointer;
		font-size: 0.9em;
		vertical-align: baseline;
		transition: background-color 0.15s ease;
		border: none;
		font-family: inherit;
		margin: 0 2px;
	}

	.entity-chip:hover {
		background-color: color-mix(in srgb, var(--color-primary) 20%, transparent);
	}

	.entity-icon {
		display: flex;
		align-items: center;
		opacity: 0.8;
	}

	.entity-text {
		font-weight: 500;
	}
</style>
