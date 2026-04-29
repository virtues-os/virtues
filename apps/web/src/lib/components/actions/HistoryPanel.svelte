<script lang="ts">
	import Badge from '$lib/components/Badge.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { FilterDef } from '$lib/components/datagrid/types';
	import { listRuns, listActions, type ActionRun, type Action } from '$lib/api/client';
	import { relativeTime } from '$lib/actions/palette';

	let runs = $state<ActionRun[]>([]);
	let actions = $state<Action[]>([]);
	let loading = $state(true);
	let err = $state<string | null>(null);

	async function load() {
		loading = true;
		err = null;
		try {
			const [rs, as] = await Promise.all([
				listRuns({ limit: 200 }),
				actions.length === 0 ? listActions() : Promise.resolve(actions)
			]);
			runs = rs;
			if (actions.length === 0) actions = as;
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
	});

	function actionName(id: string | null): string {
		if (!id) return '(deleted)';
		return actions.find((a) => a.id === id)?.name ?? id;
	}

	function duration(r: ActionRun): string {
		if (!r.completed_at) return '—';
		const start = new Date(r.started_at).getTime();
		const end = new Date(r.completed_at).getTime();
		const ms = end - start;
		if (ms < 1000) return `${ms}ms`;
		if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
		return `${(ms / 60_000).toFixed(1)}m`;
	}

	const columns: Column<ActionRun>[] = [
		{
			key: 'action_id',
			label: 'Action',
			width: '20%',
			minWidth: '120px',
			getValue: (r) => actionName(r.action_id)
		},
		{ key: 'trigger', label: 'Trigger' },
		{
			key: 'started_at',
			label: 'Started',
			getValue: (r) => relativeTime(r.started_at)
		},
		{
			key: 'completed_at',
			label: 'Duration',
			getValue: (r) => duration(r)
		},
		{
			key: 'status',
			label: 'Status',
			format: 'badge',
			badgeColors: {
				success: 'badge-green',
				error: 'badge-red',
				skipped: 'badge-gray',
				running: 'badge-yellow',
				cancelled: 'badge-gray'
			}
		},
		{
			key: 'result_summary',
			label: 'Summary',
			getValue: (r) => r.result_summary ?? r.error ?? ''
		}
	];

	const filters: FilterDef<ActionRun>[] = [
		{
			id: 'status',
			kind: 'enum',
			label: 'Status',
			field: 'status',
			options: [
				{ value: 'success', label: 'Success', badgeColor: 'badge-green' },
				{ value: 'error', label: 'Error', badgeColor: 'badge-red' },
				{ value: 'running', label: 'Running', badgeColor: 'badge-yellow' },
				{ value: 'skipped', label: 'Skipped', badgeColor: 'badge-gray' }
			]
		},
		{
			id: 'action_id',
			kind: 'async',
			label: 'Action',
			field: 'action_id',
			searchable: true,
			placeholder: 'Search actions…',
			loadOptions: async () => {
				const list = actions.length > 0 ? actions : await listActions();
				return list.map((a) => ({ value: a.id, label: a.name }));
			}
		}
	];
</script>

<section class="history-panel">
	<header class="section-header">
		<div>
			<h2>History</h2>
			<p class="subtitle">Every run across all actions, filtered by status and time.</p>
		</div>
	</header>

	<UniversalDataGrid
		items={runs}
		{columns}
		{filters}
		entityType="action-history"
		defaultViewMode="table"
		{loading}
		error={err}
		emptyIcon="ri:history-line"
		emptyMessage="No runs match the current filters."
		searchPlaceholder="Search runs…"
		pageSize={50}
		refreshInterval={5000}
		onRefresh={load}
	>
		{#snippet expandDetail(run)}
			<div class="expand-detail">
				{#if run.result_summary}
					<div class="detail-block">
						<div class="detail-label">Result</div>
						<pre>{run.result_summary}</pre>
					</div>
				{/if}
				{#if run.error}
					<div class="detail-block error-block">
						<div class="detail-label">Error</div>
						<pre>{run.error}</pre>
					</div>
				{/if}
				<div class="detail-meta">
					<span>Run: <code>{run.id}</code></span>
					{#if run.action_id}
						<span>Action: <code>{run.action_id}</code></span>
					{/if}
					<span>Records: {run.records_processed}</span>
				</div>
			</div>
		{/snippet}

		{#snippet card(run)}
			<div class="run-card">
				<div class="run-card-top">
					<span class="run-card-action">{actionName(run.action_id)}</span>
					<Badge
						variant={run.status === 'success'
							? 'success'
							: run.status === 'error'
								? 'error'
								: run.status === 'running'
									? 'info'
									: 'muted'}
					>
						{run.status}
					</Badge>
				</div>
				<div class="run-card-meta">
					<span>{relativeTime(run.started_at)}</span>
					<span class="dot-sep">·</span>
					<span>{duration(run)}</span>
					<span class="dot-sep">·</span>
					<span class="mono">{run.trigger}</span>
				</div>
				{#if run.result_summary || run.error}
					<p class="run-card-summary" class:error={run.status === 'error'}>
						{run.result_summary ?? run.error}
					</p>
				{/if}
			</div>
		{/snippet}
	</UniversalDataGrid>
</section>

<style>
	.history-panel {
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

	/* Expand detail (rendered inside the grid's expandDetail snippet) */
	.expand-detail {
		padding: 0.75rem 1rem;
		display: flex;
		flex-direction: column;
		gap: 0.625rem;
		background: var(--color-surface-elevated, #f9fafb);
	}
	.detail-label {
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-foreground-subtle, #9ca3af);
		margin-bottom: 0.25rem;
	}
	.detail-block pre {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 0.75rem;
		white-space: pre-wrap;
		word-break: break-word;
		background: var(--color-surface, #fff);
		padding: 0.5rem 0.625rem;
		border-radius: 4px;
		max-height: 200px;
		overflow-y: auto;
	}
	.error-block pre {
		color: #991b1b;
		background: #fef2f2;
	}
	.detail-meta {
		display: flex;
		gap: 1rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.detail-meta code {
		font-family: var(--font-mono, monospace);
	}

	/* Card view for runs */
	.run-card {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		padding: 0.75rem 0.875rem;
		border-radius: 8px;
		border: 1px solid var(--color-border, #e5e7eb);
		background: var(--color-surface, #fff);
	}
	.run-card:hover {
		background: var(--color-surface-elevated, #f9fafb);
	}
	.run-card-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
	}
	.run-card-action {
		font-size: 0.875rem;
		font-weight: 600;
	}
	.run-card-meta {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.dot-sep {
		opacity: 0.5;
	}
	.mono {
		font-family: var(--font-mono, monospace);
	}
	.run-card-summary {
		margin: 0;
		font-size: 0.75rem;
		line-height: 1.4;
		color: var(--color-foreground-muted, #6b7280);
		display: -webkit-box;
		line-clamp: 2;
		-webkit-line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.run-card-summary.error {
		color: #991b1b;
	}
</style>
