<!--
	OrganizationTable.svelte

	View for organizations in the wiki.
	Uses UniversalDataGrid for table/card views.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { listOrganizations, type WikiOrganizationListItem } from "$lib/wiki/api";
	import UniversalDataGrid, { type Column } from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import Icon from "$lib/components/Icon.svelte";

	let organizations = $state<WikiOrganizationListItem[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const orgTypeColors: Record<string, string> = {
		company: "badge-muted",
		employer: "badge-muted",
		school: "badge-muted",
		university: "badge-muted",
		community: "badge-muted",
		nonprofit: "badge-muted",
		government: "badge-muted",
		other: "badge-muted",
	};

	const columns: Column<WikiOrganizationListItem>[] = [
		{
			key: 'name',
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
		if (!type) return "badge-muted";
		return orgTypeColors[type.toLowerCase()] || "badge-muted";
	}

	function handleItemClick(org: WikiOrganizationListItem) {
		const route = `/org/${org.id}`;
		windowShellStore.openTabFromRoute(route);
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
			<span class="card-name">{org.name}</span>
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

</style>
