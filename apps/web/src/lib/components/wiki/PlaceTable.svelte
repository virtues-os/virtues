<!--
	PlaceTable.svelte

	View for places in the wiki.
	Uses UniversalDataGrid for table/card views.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { spaceStore } from "$lib/stores/space.svelte";
	import { listPlaces, type WikiPlaceListItem } from "$lib/wiki/api";
	import UniversalDataGrid, { type Column } from "$lib/components/UniversalDataGrid.svelte";
	import Icon from "$lib/components/Icon.svelte";

	let places = $state<WikiPlaceListItem[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const categoryColors: Record<string, string> = {
		home: "badge-blue",
		work: "badge-green",
		office: "badge-green",
		museum: "badge-purple",
		restaurant: "badge-orange",
		cafe: "badge-orange",
		gym: "badge-green",
		other: "badge-gray",
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
			key: 'visit_count',
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
		if (!category) return "badge-gray";
		return categoryColors[category.toLowerCase()] || "badge-gray";
	}

	function handleItemClick(place: WikiPlaceListItem) {
		const route = `/place/${place.id}`;
		spaceStore.openTabFromRoute(route);
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
			{#if place.visit_count !== undefined && place.visit_count !== null}
				<span class="visits-text">{place.visit_count} visits</span>
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

	/* Badges */
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
