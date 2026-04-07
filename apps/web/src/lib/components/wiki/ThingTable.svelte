<!--
	ThingTable.svelte

	View for things in the wiki.
	Uses UniversalDataGrid for table/card views.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { spaceStore } from "$lib/stores/space.svelte";
	import { listThings, type WikiThingListItem } from "$lib/wiki/api";
	import UniversalDataGrid, { type Column } from "$lib/components/UniversalDataGrid.svelte";
	import Icon from "$lib/components/Icon.svelte";

	let things = $state<WikiThingListItem[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const categoryColors: Record<string, string> = {
		pet: "badge-green",
		project: "badge-blue",
		concept: "badge-purple",
		hobby: "badge-orange",
		tool: "badge-gray",
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
		if (!category) return "badge-gray";
		return categoryColors[category.toLowerCase()] || "badge-gray";
	}

	function handleItemClick(thing: WikiThingListItem) {
		const route = `/thing/${thing.id}`;
		spaceStore.openTabFromRoute(route);
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

	.badge {
		display: inline-flex;
		align-items: center;
		padding: 0.125rem 0.5rem;
		font-size: 0.75rem;
		font-weight: 500;
		border-radius: 9999px;
		white-space: nowrap;
		text-transform: capitalize;
	}

	.badge-gray {
		background: color-mix(in srgb, var(--color-foreground) 10%, transparent);
		color: var(--color-foreground-muted);
	}

	.badge-blue {
		background: color-mix(in srgb, #3b82f6 15%, transparent);
		color: #2563eb;
	}

	.badge-green {
		background: color-mix(in srgb, #22c55e 15%, transparent);
		color: #16a34a;
	}

	.badge-purple {
		background: color-mix(in srgb, #8b5cf6 15%, transparent);
		color: #7c3aed;
	}

	.badge-orange {
		background: color-mix(in srgb, #f97316 15%, transparent);
		color: #ea580c;
	}
</style>
