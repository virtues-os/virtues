<!--
	PersonTable.svelte

	Attio-style view for managing people in the wiki.
	Uses UniversalDataGrid for table/card views.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { spaceStore } from "$lib/stores/space.svelte";
	import { listPeople, type WikiPersonListItem } from "$lib/wiki/api";
	import UniversalDataGrid, { type Column } from "$lib/components/UniversalDataGrid.svelte";

	// State
	let people = $state<WikiPersonListItem[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Relationship badge colors
	const relationshipColors: Record<string, string> = {
		"self": "badge-purple",
		"friend": "badge-blue",
		"family": "badge-pink",
		"colleague": "badge-green",
		"mentor": "badge-orange",
		"acquaintance": "badge-gray",
		"partner": "badge-pink",
		"client": "badge-blue",
	};

	// Column definitions
	const columns: Column<WikiPersonListItem>[] = [
		{
			key: 'canonical_name',
			label: 'Name',
			icon: 'ri:user-line',
			width: '50%',
			minWidth: '200px',
		},
		{
			key: 'relationship_category',
			label: 'Relationship',
			icon: 'ri:heart-line',
			width: '25%',
			minWidth: '120px',
			format: 'badge',
			badgeColors: relationshipColors,
		},
		{
			key: 'last_interaction',
			label: 'Last Interaction',
			icon: 'ri:calendar-line',
			width: '25%',
			minWidth: '140px',
			hideOnMobile: true,
			getValue: (item) => formatRelativeDate(item.last_interaction),
		},
	];

	async function loadPeople() {
		loading = true;
		error = null;
		try {
			people = await listPeople();
		} catch (e) {
			console.error('Failed to load people:', e);
			error = e instanceof Error ? e.message : 'Failed to load people';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadPeople();
	});

	function formatRelativeDate(dateStr?: string | null): string | null {
		if (!dateStr) return null;
		const date = new Date(dateStr);
		if (Number.isNaN(date.getTime())) return null;
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		if (diffMs < 0) return "Upcoming";
		const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

		if (diffDays === 0) return "Today";
		if (diffDays === 1) return "Yesterday";
		if (diffDays < 7) return `${diffDays} days ago`;
		if (diffDays < 14) return "About 1 week ago";
		if (diffDays < 30) return `${Math.floor(diffDays / 7)} weeks ago`;
		if (diffDays < 60) return "About 1 month ago";
		if (diffDays < 365) return `${Math.floor(diffDays / 30)} months ago`;
		return `${Math.floor(diffDays / 365)} years ago`;
	}

	function getRelationshipClass(rel?: string | null): string {
		if (!rel) return "badge-gray";
		return relationshipColors[rel.toLowerCase()] || "badge-gray";
	}

	function handleItemClick(person: WikiPersonListItem) {
		const route = `/person/${person.id}`;
		spaceStore.openTabFromRoute(route);
	}
</script>

<UniversalDataGrid
	items={people}
	{columns}
	entityType="person"
	{loading}
	{error}
	emptyIcon="ri:user-add-line"
	emptyMessage="No people yet"
	loadingMessage="Loading people..."
	searchPlaceholder="Search people..."
	onItemClick={handleItemClick}
	onRetry={loadPeople}
>
	<!-- Custom table row with avatar -->
	{#snippet tableRow(person: WikiPersonListItem)}
		<td class="col-name">
			<div class="name-cell">
				{#if person.picture}
					<img src={person.picture} alt={person.canonical_name} class="avatar-img" />
				{:else}
					<div class="avatar">
						{person.canonical_name.charAt(0).toUpperCase()}
					</div>
				{/if}
				<span class="name-text">{person.canonical_name}</span>
			</div>
		</td>
		<td class="col-relationship">
			{#if person.relationship_category}
				<span class="badge {getRelationshipClass(person.relationship_category)}">
					{person.relationship_category}
				</span>
			{:else}
				<span class="empty-cell">—</span>
			{/if}
		</td>
		<td class="col-last-interaction hide-mobile">
			{#if person.last_interaction}
				<span class="date-text">{formatRelativeDate(person.last_interaction)}</span>
			{:else}
				<span class="empty-cell">—</span>
			{/if}
		</td>
	{/snippet}

	<!-- Custom card with avatar -->
	{#snippet card(person: WikiPersonListItem)}
		<div class="card-content">
			{#if person.picture}
				<img src={person.picture} alt={person.canonical_name} class="avatar-img avatar-lg" />
			{:else}
				<div class="avatar avatar-lg">
					{person.canonical_name.charAt(0).toUpperCase()}
				</div>
			{/if}
			<span class="card-name">{person.canonical_name}</span>
			{#if person.relationship_category}
				<span class="badge {getRelationshipClass(person.relationship_category)}">
					{person.relationship_category}
				</span>
			{/if}
		</div>
	{/snippet}
</UniversalDataGrid>

<style>
	/* Table row styles */
	.name-cell {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.avatar {
		width: 28px;
		height: 28px;
		border-radius: 50%;
		background: linear-gradient(135deg, var(--color-primary), color-mix(in srgb, var(--color-primary) 70%, #000));
		color: white;
		font-size: 0.75rem;
		font-weight: 600;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.avatar-lg {
		width: 56px;
		height: 56px;
		font-size: 1.25rem;
	}

	.avatar-img {
		width: 28px;
		height: 28px;
		border-radius: 50%;
		object-fit: cover;
		flex-shrink: 0;
	}

	.avatar-img.avatar-lg {
		width: 56px;
		height: 56px;
	}

	.name-text {
		font-weight: 500;
		color: var(--color-foreground);
	}

	/* Card styles */
	.card-content {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		text-align: center;
	}

	.card-name {
		font-weight: 600;
		font-size: 0.9375rem;
		color: var(--color-foreground);
		line-height: 1.3;
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

	.badge-pink {
		background: color-mix(in srgb, #ec4899 15%, transparent);
		color: #db2777;
	}

	.badge-orange {
		background: color-mix(in srgb, #f97316 15%, transparent);
		color: #ea580c;
	}

	.date-text {
		color: var(--color-foreground-muted);
		font-size: 0.8125rem;
	}

	.empty-cell {
		color: var(--color-foreground-subtle);
	}

	/* Column classes */
	.col-name {
		width: 50%;
		min-width: 200px;
		padding: 0.625rem 0.75rem;
		padding-left: 0;
	}

	.col-relationship {
		width: 25%;
		min-width: 120px;
		padding: 0.625rem 0.75rem;
	}

	.col-last-interaction {
		width: 25%;
		min-width: 140px;
		padding: 0.625rem 0.75rem;
		padding-right: 0;
	}

	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}

		.col-name {
			width: 60%;
		}

		.col-relationship {
			width: 40%;
		}
	}
</style>
