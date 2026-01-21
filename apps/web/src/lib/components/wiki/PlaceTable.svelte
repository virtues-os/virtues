<!--
	PlaceTable.svelte

	Table view for places in the wiki.
-->

<script lang="ts">
	import { goto } from "$app/navigation";
	import { getAllPlaces } from "$lib/wiki";
	import type { PlacePage } from "$lib/wiki/types";
	import "iconify-icon";

	// Reactive list of places
	let places = $state(getAllPlaces());

	// Place type labels
	const placeTypeLabels: Record<string, string> = {
		home: "Home",
		work: "Work",
		"third-place": "Third Place",
		transit: "Transit",
		travel: "Travel",
		other: "Other",
	};

	// Place type badge colors
	const placeTypeColors: Record<string, string> = {
		home: "badge-blue",
		work: "badge-green",
		"third-place": "badge-purple",
		transit: "badge-gray",
		travel: "badge-orange",
		other: "badge-gray",
	};

	function getPlaceTypeLabel(type: string): string {
		return placeTypeLabels[type] || type;
	}

	function getPlaceTypeClass(type: string): string {
		return placeTypeColors[type] || "badge-gray";
	}

	// Handle row click - navigate to place page
	function handleRowClick(place: PlacePage) {
		goto(`/wiki/${place.slug}`);
	}
</script>

<div class="table-wrapper">
	<!-- Toolbar -->
	<div class="table-toolbar">
		<div class="toolbar-left">
			<button class="toolbar-btn">
				<iconify-icon icon="ri:arrow-up-down-line" width="14"></iconify-icon>
				Sort
			</button>
			<button class="toolbar-btn">
				<iconify-icon icon="ri:filter-3-line" width="14"></iconify-icon>
				Filter
			</button>
		</div>
	</div>

	<!-- Table -->
	<div class="table-container">
		<table class="data-table">
			<thead>
				<tr>
					<th class="col-name">
						<iconify-icon icon="ri:map-pin-line" width="14"></iconify-icon>
						Name
					</th>
					<th class="col-type">
						<iconify-icon icon="ri:building-line" width="14"></iconify-icon>
						Type
					</th>
					<th class="col-city">
						<iconify-icon icon="ri:map-2-line" width="14"></iconify-icon>
						City
					</th>
					<th class="col-visits">
						<iconify-icon icon="ri:footprint-line" width="14"></iconify-icon>
						Visits
					</th>
				</tr>
			</thead>
			<tbody>
				{#each places as place}
					<tr class="data-row" onclick={() => handleRowClick(place)}>
						<td class="col-name">
							<span class="name-text">{place.title}</span>
						</td>
						<td class="col-type">
							<span class="badge {getPlaceTypeClass(place.placeType)}">
								{getPlaceTypeLabel(place.placeType)}
							</span>
						</td>
						<td class="col-city">
							{#if place.city}
								<span class="city-text">{place.city}</span>
							{:else}
								<span class="empty-cell">—</span>
							{/if}
						</td>
						<td class="col-visits">
							{#if place.visitCount}
								<span class="visits-text">{place.visitCount}</span>
							{:else}
								<span class="empty-cell">—</span>
							{/if}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>

	<!-- Empty state -->
	{#if places.length === 0}
		<div class="empty-state">
			<iconify-icon icon="ri:map-pin-add-line" width="32"></iconify-icon>
			<p>No places yet</p>
		</div>
	{/if}
</div>

<style>
	.table-wrapper {
		width: 100%;
		padding: 0 2rem;
		overflow: visible;
	}

	/* Toolbar */
	.table-toolbar {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.5rem 0;
		position: relative;
	}

	.table-toolbar::after {
		content: "";
		position: absolute;
		left: -2rem;
		right: -2rem;
		bottom: 0;
		height: 1px;
		background: var(--color-border);
	}

	.toolbar-left {
		display: flex;
		gap: 0.5rem;
	}

	.toolbar-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.625rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.toolbar-btn:hover {
		background: var(--color-background-hover);
		color: var(--color-foreground);
	}

	/* Table container */
	.table-container {
		overflow: visible;
	}

	.data-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8125rem;
		overflow: visible;
	}

	/* Header */
	thead tr {
		background: transparent;
		position: relative;
	}

	thead tr::after {
		content: "";
		position: absolute;
		left: -2rem;
		right: -2rem;
		bottom: 0;
		height: 1px;
		background: var(--color-border);
	}

	th {
		text-align: left;
		font-weight: 500;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		padding: 0.625rem 0.75rem;
		white-space: nowrap;
	}

	th:first-child {
		padding-left: 0;
	}

	th:last-child {
		padding-right: 0;
	}

	th iconify-icon {
		vertical-align: -2px;
		margin-right: 0.375rem;
		opacity: 0.7;
	}

	/* Column widths */
	.col-name {
		width: 40%;
		min-width: 200px;
	}

	.col-type {
		width: 20%;
		min-width: 120px;
	}

	.col-city {
		width: 25%;
		min-width: 140px;
	}

	.col-visits {
		width: 15%;
		min-width: 80px;
	}

	/* Data rows */
	td {
		padding: 0.5rem 0.75rem;
		color: var(--color-foreground);
		vertical-align: middle;
	}

	td:first-child {
		padding-left: 0;
	}

	td:last-child {
		padding-right: 0;
	}

	.data-row {
		cursor: pointer;
		transition: background-color 0.1s ease;
		position: relative;
	}

	.data-row::after {
		content: "";
		position: absolute;
		left: -2rem;
		right: -2rem;
		bottom: 0;
		height: 1px;
		background: var(--color-border);
	}

	.data-row:hover {
		background: var(--color-background-hover);
	}

	.name-text {
		font-weight: 500;
		color: var(--color-foreground);
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

	/* Text styles */
	.city-text {
		color: var(--color-foreground);
	}

	.visits-text {
		color: var(--color-foreground-muted);
	}

	.empty-cell {
		color: var(--color-foreground-subtle);
	}

	/* Empty state */
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
		padding: 3rem 2rem;
		color: var(--color-foreground-muted);
	}

	.empty-state iconify-icon {
		opacity: 0.5;
	}

	.empty-state p {
		margin: 0;
		font-size: 0.875rem;
	}

	/* Responsive */
	@media (max-width: 768px) {
		.col-city,
		.col-visits {
			display: none;
		}

		.col-name {
			width: 60%;
		}

		.col-type {
			width: 40%;
		}
	}
</style>
