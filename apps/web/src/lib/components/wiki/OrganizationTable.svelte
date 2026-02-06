<!--
	OrganizationTable.svelte

	View for organizations in the wiki.
	Uses UniversalDataGrid for table/card views.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { spaceStore } from "$lib/stores/space.svelte";
	import { listOrganizations, type WikiOrganizationListItem } from "$lib/wiki/api";
	import UniversalDataGrid, { type Column } from "$lib/components/UniversalDataGrid.svelte";
	import Icon from "$lib/components/Icon.svelte";

	let organizations = $state<WikiOrganizationListItem[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const orgTypeColors: Record<string, string> = {
		company: "badge-green",
		employer: "badge-green",
		school: "badge-blue",
		university: "badge-blue",
		community: "badge-purple",
		nonprofit: "badge-purple",
		government: "badge-orange",
		other: "badge-gray",
	};

	const columns: Column<WikiOrganizationListItem>[] = [
		{
			key: 'canonical_name',
			label: 'Name',
			icon: 'ri:building-2-line',
			width: '50%',
			minWidth: '200px',
		},
		{
			key: 'organization_type',
			label: 'Type',
			icon: 'ri:price-tag-3-line',
			width: '25%',
			minWidth: '120px',
			format: 'badge',
			badgeColors: orgTypeColors,
		},
		{
			key: 'relationship_type',
			label: 'Relationship',
			icon: 'ri:links-line',
			width: '25%',
			minWidth: '120px',
			hideOnMobile: true,
		},
	];

	async function loadOrganizations() {
		loading = true;
		error = null;
		try {
			organizations = await listOrganizations();
		} catch (e) {
			console.error('Failed to load organizations:', e);
			error = e instanceof Error ? e.message : 'Failed to load organizations';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadOrganizations();
	});

	function getOrgTypeClass(type?: string | null): string {
		if (!type) return "badge-gray";
		return orgTypeColors[type.toLowerCase()] || "badge-gray";
	}

	function handleItemClick(org: WikiOrganizationListItem) {
		const route = `/org/${org.id}`;
		spaceStore.openTabFromRoute(route);
	}
</script>

<UniversalDataGrid
	items={organizations}
	{columns}
	entityType="org"
	{loading}
	{error}
	emptyIcon="ri:building-2-line"
	emptyMessage="No organizations yet"
	loadingMessage="Loading organizations..."
	searchPlaceholder="Search organizations..."
	onItemClick={handleItemClick}
	onRetry={loadOrganizations}
>
	<!-- Custom card -->
	{#snippet card(org: WikiOrganizationListItem)}
		<div class="card-content">
			<div class="org-icon">
				<Icon icon="ri:building-2-line" width="24" />
			</div>
			<span class="card-name">{org.canonical_name}</span>
			{#if org.organization_type}
				<span class="badge {getOrgTypeClass(org.organization_type)}">
					{org.organization_type}
				</span>
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

	.org-icon {
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
