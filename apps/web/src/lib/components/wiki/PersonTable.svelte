<!--
	PersonTable.svelte

	Attio-style view for managing people in the wiki.
	Uses UniversalDataGrid for table/card views.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { listPeople, type WikiPersonListItem } from "$lib/wiki/api";
	import UniversalDataGrid, { type Column } from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import PersonPicture from "./PersonPicture.svelte";

	// State
	let people = $state<WikiPersonListItem[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const relationshipColors: Record<string, string> = {
		"self": "badge-muted",
		"friend": "badge-muted",
		"family": "badge-muted",
		"colleague": "badge-muted",
		"mentor": "badge-muted",
		"acquaintance": "badge-muted",
		"partner": "badge-muted",
		"client": "badge-muted",
	};

	// Column definitions
	const columns: Column<WikiPersonListItem>[] = [
		{
			key: 'name',
			label: 'Name',
			icon: 'mynaui:user',
			width: '50%',
			minWidth: '200px',
		},
		{
			key: 'relationship_category',
			label: 'Relationship',
			icon: 'mynaui:heart',
			width: '25%',
			minWidth: '120px',
			format: 'badge',
			badgeColors: relationshipColors,
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
		if (!rel) return "badge-muted";
		return relationshipColors[rel.toLowerCase()] || "badge-muted";
	}

	function handleItemClick(person: WikiPersonListItem) {
		const route = `/person/${person.id}`;
		windowShellStore.openTabFromRoute(route);
	}
</script>

<UniversalDataGrid
	items={people}
	{columns}
	entityType="person"
	{loading}
	{error}
	emptyIcon="mynaui:user-plus"
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
				<PersonPicture name={person.name} picture={person.picture} size={28} />
				<span class="name-text">{person.name}</span>
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
	{/snippet}

	<!-- Custom card with avatar -->
	{#snippet card(person: WikiPersonListItem)}
		<div class="card-content">
			<PersonPicture name={person.name} picture={person.picture} size={56} />
			<span class="card-name">{person.name}</span>
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
