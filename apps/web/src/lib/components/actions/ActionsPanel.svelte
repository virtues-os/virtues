<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { FilterDef } from '$lib/components/datagrid/types';
	import {
		listActions,
		listActionRuns,
		type Action,
		type ActionRun
	} from '$lib/api/client';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { describeSchedule, relativeTime } from '$lib/actions/palette';
	import ActionCard from './ActionCard.svelte';
	import NewActionModal from './NewActionModal.svelte';

	let actions = $state<Action[]>([]);
	let pulseByAction = $state<Record<string, ActionRun[]>>({});
	let lastSuccessByAction = $state<Record<string, ActionRun | null>>({});
	let loading = $state(true);
	let err = $state<string | null>(null);
	let newModalOpen = $state(false);

	async function load() {
		loading = true;
		err = null;
		try {
			actions = await listActions();
			void Promise.all(
				actions.map(async (a) => {
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

	function openCard(a: Action) {
		spaceStore.openAside({
			type: 'action',
			label: a.name,
			route: `/action/${a.id}`,
			icon: 'ri:flashlight-line'
		});
	}

	function lastRunStatus(action: Action): string {
		const lr = action.last_run;
		if (!lr) return '—';
		return lr.status ?? '—';
	}

	const columns: Column<Action>[] = [
		{ key: 'name', label: 'Name', width: '30%', minWidth: '140px' },
		{
			key: 'owner',
			label: 'Owner',
			format: 'badge',
			getValue: (a) => a.owner
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
		},
		{
			key: 'enabled',
			label: 'Status',
			format: 'badge',
			getValue: (a) => lastRunStatus(a),
			badgeColors: {
				success: 'badge-green',
				error: 'badge-red',
				skipped: 'badge-gray',
				running: 'badge-yellow',
				'—': 'badge-gray'
			}
		}
	];

	const filters: FilterDef<Action>[] = [
		{
			id: 'owner',
			kind: 'multi',
			label: 'Owner',
			options: [
				{ value: 'system', label: 'System' },
				{ value: 'user', label: 'User' }
			],
			predicate: (a, v) => Array.isArray(v) && v.includes(a.owner)
		},
		{
			id: 'enabled',
			kind: 'enum',
			label: 'Status',
			options: [
				{ value: 'true', label: 'Enabled', badgeColor: 'badge-green' },
				{ value: 'false', label: 'Disabled', badgeColor: 'badge-gray' }
			],
			predicate: (a, v) => String(a.enabled) === v
		},
		{
			id: 'schedule_type',
			kind: 'multi',
			label: 'Trigger',
			options: [
				{ value: 'cron', label: 'Scheduled', badgeColor: 'badge-blue' },
				{ value: 'manual', label: 'Manual', badgeColor: 'badge-gray' }
			],
			predicate: (a, v) => {
				const t = a.cron_schedule ? 'cron' : 'manual';
				return Array.isArray(v) && v.includes(t);
			}
		},
		{
			id: 'last_run_status',
			kind: 'enum',
			label: 'Last run',
			options: [
				{ value: 'success', label: 'Success', badgeColor: 'badge-green' },
				{ value: 'error', label: 'Error', badgeColor: 'badge-red' },
				{ value: 'running', label: 'Running', badgeColor: 'badge-yellow' },
				{ value: 'skipped', label: 'Skipped', badgeColor: 'badge-gray' }
			],
			predicate: (a, v) => (a.last_run?.status ?? null) === v
		}
	];
</script>

<section class="actions-panel">
	<header class="section-header">
		<div>
			<h2>Actions</h2>
			<p class="subtitle">Scheduled pipelines and automations your system runs for you.</p>
		</div>
		<div class="header-actions">
			<Button variant="primary" onclick={() => (newModalOpen = true)}>
				<Icon icon="ri:add-line" width="14" /> New
			</Button>
		</div>
	</header>

	<UniversalDataGrid
		items={actions}
		{columns}
		{filters}
		entityType="actions"
		defaultViewMode="table"
		gridMinWidth="340px"
		{loading}
		error={err}
		emptyIcon="ri:flashlight-line"
		emptyMessage="No actions yet."
		searchPlaceholder="Search actions…"
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

<NewActionModal
	open={newModalOpen}
	onClose={() => (newModalOpen = false)}
	onCreated={() => {
		newModalOpen = false;
		void load();
	}}
/>

<style>
	.actions-panel {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	.section-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
		flex-wrap: wrap;
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
	.header-actions {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}
</style>
