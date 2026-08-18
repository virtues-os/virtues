<!--
	PlaceTable.svelte

	View for places in the wiki.
	Uses UniversalDataGrid for table/card views.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { listPlaces, type WikiPlaceListItem } from "$lib/wiki/api";
	import UniversalDataGrid, { type Column } from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import Icon from "$lib/components/Icon.svelte";

	let places = $state<WikiPlaceListItem[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const categoryColors: Record<string, string> = {
		home: "badge-muted",
		work: "badge-muted",
		office: "badge-muted",
		museum: "badge-muted",
		restaurant: "badge-muted",
		cafe: "badge-muted",
		gym: "badge-muted",
		other: "badge-muted",
	};

	const columns: Column<WikiPlaceListItem>[] = [
		{
			key: 'name',
			label: 'Name',
			icon: 'ri:map-pin-line',
			width: '30%',
			minWidth: '160px',
		},
		{
			key: 'category',
			label: 'Category',
			icon: 'ri:price-tag-3-line',
			width: '15%',
			minWidth: '100px',
			format: 'badge',
			badgeColors: categoryColors,
		},
		{
			key: 'address',
			label: 'Address',
			icon: 'ri:map-2-line',
			width: '40%',
			minWidth: '200px',
			hideOnMobile: true,
		},
		{
			key: 'ref_count',
			label: 'Visits',
			icon: 'ri:footprint-line',
			width: '15%',
			minWidth: '80px',
			hideOnMobile: true,
		},
	];

	async function loadPlaces() {
		loading = true;
		error = null;
		try {
			places = await listPlaces();
		} catch (e) {
			console.error('Failed to load places:', e);
			error = e instanceof Error ? e.message : 'Failed to load places';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadPlaces();
	});

	function getCategoryClass(category?: string | null): string {
		if (!category) return "badge-muted";
		return categoryColors[category.toLowerCase()] || "badge-muted";
	}

	function handleItemClick(place: WikiPlaceListItem) {
		const route = `/place/${place.id}`;
		windowShellStore.openTabFromRoute(route);
	}
</script>

<UniversalDataGrid
	items={places}
	{columns}
	entityType="place"
	{loading}
	{error}
	emptyIcon="ri:map-pin-add-line"
	emptyMessage="No places yet"
	loadingMessage="Loading places..."
	searchPlaceholder="Search places..."
	onItemClick={handleItemClick}
	onRetry={loadPlaces}
>
	<!-- Custom card -->
	{#snippet card(place: WikiPlaceListItem)}
		<div class="card-content">
			<div class="place-icon">
				<Icon icon="ri:map-pin-line" width="24" />
			</div>
			<span class="card-name">{place.name}</span>
			{#if place.category}
				<span class="badge {getCategoryClass(place.category)}">
					{place.category}
				</span>
			{/if}
			{#if place.ref_count !== undefined && place.ref_count !== null}
				<span class="visits-text">{place.ref_count} visits</span>
			{/if}
		</div>
	{/snippet}
</UniversalDataGrid>

<style>
	/* Card styles */
	.card-content {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		text-align: center;
	}

	.place-icon {
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

	.visits-text {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

</style>
