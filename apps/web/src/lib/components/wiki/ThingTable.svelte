<!--
	ThingTable.svelte

	View for things in the wiki.
	Uses UniversalDataGrid for table/card views.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { listThings, type WikiThingListItem } from "$lib/wiki/api";
	import UniversalDataGrid, { type Column } from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import Icon from "$lib/components/Icon.svelte";

	let things = $state<WikiThingListItem[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const categoryColors: Record<string, string> = {
		pet: "badge-muted",
		project: "badge-muted",
		concept: "badge-muted",
		hobby: "badge-muted",
		tool: "badge-muted",
	};

	const columns: Column<WikiThingListItem>[] = [
		{
			key: 'name',
			label: 'Name',
			icon: 'ri:lightbulb-line',
			width: '40%',
			minWidth: '200px',
		},
		{
			key: 'category',
			label: 'Category',
			icon: 'ri:price-tag-3-line',
			width: '25%',
			minWidth: '120px',
			format: 'badge',
			badgeColors: categoryColors,
		},
		{
			key: 'description',
			label: 'Description',
			icon: 'ri:text',
			width: '35%',
			minWidth: '150px',
			hideOnMobile: true,
		},
	];

	async function loadThings() {
		loading = true;
		error = null;
		try {
			things = await listThings();
		} catch (e) {
			console.error('Failed to load things:', e);
			error = e instanceof Error ? e.message : 'Failed to load things';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadThings();
	});

	function getCategoryClass(category?: string | null): string {
		if (!category) return "badge-muted";
		return categoryColors[category.toLowerCase()] || "badge-muted";
	}

	function handleItemClick(thing: WikiThingListItem) {
		const route = `/thing/${thing.id}`;
		windowShellStore.openTabFromRoute(route);
	}
</script>

<UniversalDataGrid
	items={things}
	{columns}
	entityType="thing"
	{loading}
	{error}
	emptyIcon="ri:lightbulb-line"
	emptyMessage="No things yet"
	loadingMessage="Loading things..."
	searchPlaceholder="Search things..."
	onItemClick={handleItemClick}
	onRetry={loadThings}
>
	{#snippet card(thing: WikiThingListItem)}
		<div class="card-content">
			<div class="thing-icon">
				<Icon icon="ri:lightbulb-line" width="24" />
			</div>
			<span class="card-name">{thing.name}</span>
			{#if thing.category}
				<span class="badge {getCategoryClass(thing.category)}">
					{thing.category}
				</span>
			{/if}
		</div>
	{/snippet}
</UniversalDataGrid>

<style>
	.card-content {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		text-align: center;
	}

	.thing-icon {
		width: 48px;
		height: 48px;
		border-radius: 12px;
		background: color-mix(in srgb, var(--color-primary) 15%, transparent);
		color: var(--color-primary);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.card-name {
		font-weight: 600;
		font-size: 0.9375rem;
		color: var(--color-foreground);
		line-height: 1.3;
	}

</style>
