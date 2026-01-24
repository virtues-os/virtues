<script lang="ts">
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import "iconify-icon";

	let { displayName, entityId } = $props<{
		displayName: string;
		entityId: string;
	}>();

	function getEntityIcon(id: string): string {
		if (id.startsWith("person_")) return "ri:user-line";
		if (id.startsWith("place_")) return "ri:map-pin-line";
		if (id.startsWith("org_")) return "ri:building-line";
		if (id.startsWith("file_")) return "ri:file-line";
		if (id.startsWith("page_")) return "ri:file-text-line";
		if (id.startsWith("thing_")) return "ri:box-3-line";
		return "ri:links-line";
	}

	function getEntityRoute(id: string): string {
		if (id.startsWith("person_")) return `/wiki/people/${id}`;
		if (id.startsWith("place_")) return `/wiki/places/${id}`;
		if (id.startsWith("org_")) return `/wiki/orgs/${id}`;
		if (id.startsWith("thing_")) return `/wiki/things/${id}`;
		if (id.startsWith("file_")) return `/data/drive?file=${id}`;
		if (id.startsWith("page_")) return `/pages/${id}`;
		return `/wiki/${id}`;
	}

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		workspaceStore.openTabFromRoute(getEntityRoute(entityId), {
			forceNew: true,
			preferEmptyPane: true,
		});
	}
</script>

<button class="entity-chip" onclick={handleClick} title="View {displayName}">
	<span class="entity-icon">
		<iconify-icon icon={getEntityIcon(entityId)} width="14"></iconify-icon>
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
