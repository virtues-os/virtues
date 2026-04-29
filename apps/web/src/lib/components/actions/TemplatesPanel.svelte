<script lang="ts">
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { listActions, listActionRuns, type Action, type ActionRun } from '$lib/api/client';
	import { describeSchedule, relativeTime } from '$lib/actions/palette';
	import { descriptionFor } from '$lib/actions/descriptions';
	import ActionCard from './ActionCard.svelte';

	let actions = $state<Action[]>([]);
	let pulseByAction = $state<Record<string, ActionRun[]>>({});
	let lastSuccessByAction = $state<Record<string, ActionRun | null>>({});
	let loading = $state(true);
	let err = $state<string | null>(null);

	async function load() {
		loading = true;
		err = null;
		try {
			actions = await listActions();
			void Promise.all(
				actions
					.filter((a) => a.owner === 'user')
					.map(async (a) => {
						try {
							const [runs, successRuns] = await Promise.all([
								listActionRuns(a.id, { limit: 10 }),
								listActionRuns(a.id, { limit: 1, status: 'success' })
							]);
							pulseByAction = { ...pulseByAction, [a.id]: runs };
							lastSuccessByAction = {
								...lastSuccessByAction,
								[a.id]: successRuns[0] ?? null
							};
						} catch {
							// decorative
						}
					})
			);
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
	});

	const templates = $derived(actions.filter((a) => a.owner === 'user'));

	function openCard(a: Action) {
		spaceStore.openAside({
			type: 'action',
			label: a.name,
			route: `/action/${a.id}`,
			icon: 'ri:flashlight-line'
		});
	}

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
			getValue: (a) => a.last_run?.started_at ? relativeTime(a.last_run.started_at) : '—'
		}
	];
</script>

<section class="templates-panel">
	<header class="section-header">
		<div>
			<h2>Templates</h2>
			<p class="subtitle">
				Customizable blueprints seeded for you. Tune the prompt, schedule, or memory.
			</p>
		</div>
	</header>

	<UniversalDataGrid
		items={templates}
		{columns}
	entityType="templates"
	defaultViewMode="grid"
	gridMinWidth="340px"
	{loading}
	error={err}
	emptyIcon="ri:stack-line"
	emptyMessage="No templates available."
	searchPlaceholder="Search templates…"
	pageSize={50}
	onItemClick={openCard}
>
	{#snippet card(action)}
		<ActionCard
			{action}
			lastRun={action.last_run}
			lastSuccess={lastSuccessByAction[action.id] ?? null}
			pulseRuns={pulseByAction[action.id] ?? []}
		/>
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
