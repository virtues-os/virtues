<script lang="ts">
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { listActions, type Action } from '$lib/api/client';
	import { describeSchedule, relativeTime } from '$lib/actions/palette';
	import { descriptionFor } from '$lib/actions/descriptions';
	import TemplateCard from './TemplateCard.svelte';
	import { loadCard } from '$lib/action-views';

	let actions = $state<Action[]>([]);
	let loading = $state(true);
	let err = $state<string | null>(null);

	async function load() {
		loading = true;
		err = null;
		try {
			actions = await listActions();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
	});

	// Templates are user-owned actions — the customizable blueprints the user
	// has defined (or seeded copies thereof). System-managed fan-out actions
	// live in ActionsPanel, not here.
	const templates = $derived(actions.filter((a) => a.owner === 'user'));

	function openCard(a: Action) {
		windowShellStore.openAside({
			type: 'action',
			label: a.name,
			route: `/action/${a.id}`,
			icon: 'ri:flashlight-line'
		});
	}

	// Columns drive table view + search index. The grid uses the TemplateCard
	// snippet, but search still scans these columns.
	const columns: Column<Action>[] = [
		{ key: 'name', label: 'Name', width: '30%', minWidth: '140px' },
		{
			key: 'agent',
			label: 'Description',
			getValue: (a) => descriptionFor(a) ?? ''
		},
		{
			key: 'cron_schedule',
			label: 'Schedule',
			getValue: (a) => describeSchedule(a.cron_schedule)
		},
		{
			key: 'id',
			label: 'Last run',
			getValue: (a) => (a.last_run?.started_at ? relativeTime(a.last_run.started_at) : '—')
		}
	];
</script>

<section class="templates-panel">
	<header class="section-header">
		<div>
			<h2>Templates</h2>
			<p class="subtitle">
				Starting points. Each template is a recipe — pick one, make it yours,
				and it becomes an action that runs.
			</p>
		</div>
	</header>

	<UniversalDataGrid
		items={templates}
		{columns}
		entityType="templates"
		defaultViewMode="grid"
		gridMinWidth="280px"
		{loading}
		error={err}
		emptyIcon="ri:stack-line"
		emptyMessage="No templates yet."
		searchPlaceholder="Search templates…"
		pageSize={50}
		onItemClick={openCard}
	>
		{#snippet card(action)}
			{@const viewName =
				typeof action.config?.view === 'object' &&
				action.config.view !== null &&
				typeof (action.config.view as { name?: unknown }).name === 'string'
					? ((action.config.view as { name: string }).name)
					: null}
			{@const CustomCard = loadCard(viewName)}
			{#if CustomCard}
				<!-- view-runtime override: action declared `config.view.name` and a
				     matching Card.svelte exists in `actions/<name>/ui/`. -->
				<CustomCard {action} onclick={openCard} />
			{:else}
				<TemplateCard {action} onclick={openCard} />
			{/if}
		{/snippet}
	</UniversalDataGrid>
</section>

<style>
	.templates-panel {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	.section-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
	}
	.section-header h2 {
		margin: 0;
		font-size: 1.25rem;
		font-weight: 600;
	}
	.subtitle {
		margin: 0.125rem 0 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
</style>
