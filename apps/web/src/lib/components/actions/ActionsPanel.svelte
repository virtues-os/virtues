<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { FilterDef } from '$lib/components/datagrid/types';
	import {
		listActions,
		listActionRuns,
		adminReconcile,
		type Action,
		type ActionRun
	} from '$lib/api/client';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { describeSchedule, relativeTime } from '$lib/actions/palette';
	import ActionCard from './ActionCard.svelte';
	import GitImportModal from './GitImportModal.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';

	let actions = $state<Action[]>([]);
	let pulseByAction = $state<Record<string, ActionRun[]>>({});
	let lastSuccessByAction = $state<Record<string, ActionRun | null>>({});
	let loading = $state(true);
	let err = $state<string | null>(null);
	let newMenuOpen = $state(false);
	let gitImportOpen = $state(false);
	let reconciling = $state(false);
	let reconcileMsg = $state<string | null>(null);

	function startChatFlow() {
		newMenuOpen = false;
		spaceStore.openTabFromRoute('/chat', { forceNew: true });
	}

	function startGitImportFlow() {
		newMenuOpen = false;
		gitImportOpen = true;
	}

	async function reconcile() {
		reconciling = true;
		reconcileMsg = null;
		try {
			const out = await adminReconcile();
			reconcileMsg = `${out.upserted} upserted · +${out.added.length} · −${out.removed.length}${out.restarted.length ? ` · ↻${out.restarted.length}` : ''}`;
			await load();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			reconciling = false;
		}
	}

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
			key: 'runtime',
			label: 'Runtime',
			format: 'badge',
			getValue: (a) => a.runtime
		},
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
				success: 'badge-success',
				error: 'badge-error',
				skipped: 'badge-muted',
				running: 'badge-warning',
				'—': 'badge-muted'
			}
		}
	];

	const filters: FilterDef<Action>[] = [
		{
			id: 'runtime',
			kind: 'multi',
			label: 'Runtime',
			options: [
				{ value: 'function', label: 'Function' },
				{ value: 'service', label: 'Service' },
				{ value: 'view', label: 'View' }
			],
			predicate: (a, v) => Array.isArray(v) && v.includes(a.runtime)
		},
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
				{ value: 'true', label: 'Enabled', badgeColor: 'badge-success' },
				{ value: 'false', label: 'Disabled', badgeColor: 'badge-muted' }
			],
			predicate: (a, v) => String(a.enabled) === v
		},
		{
			id: 'schedule_type',
			kind: 'multi',
			label: 'Trigger',
			options: [
				{ value: 'cron', label: 'Scheduled', badgeColor: 'badge-info' },
				{ value: 'manual', label: 'Manual', badgeColor: 'badge-muted' }
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
				{ value: 'success', label: 'Success', badgeColor: 'badge-success' },
				{ value: 'error', label: 'Error', badgeColor: 'badge-error' },
				{ value: 'running', label: 'Running', badgeColor: 'badge-warning' },
				{ value: 'skipped', label: 'Skipped', badgeColor: 'badge-muted' }
			],
			predicate: (a, v) => (a.last_run?.status ?? null) === v
		}
	];
</script>

<section class="actions-panel">
	<header class="section-header">
		<div>
			<h2>Actions</h2>
			<p class="subtitle">
				Everything Virtues can run for you. Functions fire on a schedule
				or trigger, apps stay running in the background, and views render
				straight from your data — all authored as folders under
				<code>actions/</code>.
			</p>
		</div>
		<div class="header-actions">
			{#if reconcileMsg}
				<span class="reconcile-msg">{reconcileMsg}</span>
			{/if}
			<button
				type="button"
				class="reconcile-btn"
				disabled={reconciling}
				onclick={reconcile}
				title="Re-read actions/*/manifest.toml from disk and apply changes"
			>
				<Icon icon="ri:refresh-line" width="14" />
				{reconciling ? 'Reconciling…' : 'Reconcile'}
			</button>
			<Popover bind:open={newMenuOpen} placement="bottom-end" offset={4}>
				{#snippet trigger({ toggle })}
					<button type="button" class="new-btn" onclick={toggle}>
						<Icon icon="ri:add-line" width="14" /> New
					</button>
				{/snippet}
				{#snippet children()}
					<div class="new-menu" role="menu">
						<button type="button" class="new-menu-item" role="menuitem" onclick={startChatFlow}>
							<Icon icon="ri:chat-smile-2-line" width="16" />
							<div class="new-menu-text">
								<div class="new-menu-title">From chat</div>
								<div class="new-menu-desc">Describe it in plain language</div>
							</div>
						</button>
						<button type="button" class="new-menu-item" role="menuitem" onclick={startGitImportFlow}>
							<Icon icon="ri:git-repository-line" width="16" />
							<div class="new-menu-text">
								<div class="new-menu-title">From Git</div>
								<div class="new-menu-desc">Import actions from a repo</div>
							</div>
						</button>
					</div>
				{/snippet}
			</Popover>
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

<GitImportModal
	open={gitImportOpen}
	onClose={() => (gitImportOpen = false)}
	onImported={load}
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
	.reconcile-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.375rem 0.625rem;
		font-size: 0.8125rem;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 6px;
		background: var(--color-surface, #fff);
		color: var(--color-foreground, #111827);
		cursor: pointer;
	}
	.reconcile-btn:hover:not(:disabled) {
		background: var(--color-surface-elevated, #f3f4f6);
	}
	.reconcile-btn:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.new-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.375rem 0.625rem;
		font-size: 0.8125rem;
		border: 1px solid var(--color-foreground, #111827);
		border-radius: 6px;
		background: var(--color-foreground, #111827);
		color: var(--color-surface, #fff);
		cursor: pointer;
	}
	.new-btn:hover {
		opacity: 0.88;
	}

	.new-menu {
		display: flex;
		flex-direction: column;
		min-width: 240px;
		padding: 0.25rem;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 8px;
		background: var(--color-surface, #fff);
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.08), 0 2px 4px rgba(0, 0, 0, 0.04);
	}
	.new-menu-item {
		display: flex;
		align-items: flex-start;
		gap: 0.625rem;
		padding: 0.5rem 0.625rem;
		border: none;
		border-radius: 6px;
		background: transparent;
		text-align: left;
		cursor: pointer;
		color: var(--color-foreground, inherit);
		font: inherit;
	}
	.new-menu-item:hover {
		background: var(--color-surface-elevated, #f3f4f6);
	}
	.new-menu-item :global(svg) {
		margin-top: 0.125rem;
		color: var(--color-foreground-subtle, #6b7280);
		flex-shrink: 0;
	}
	.new-menu-text {
		display: flex;
		flex-direction: column;
		gap: 0.0625rem;
		min-width: 0;
	}
	.new-menu-title {
		font-size: 0.8125rem;
		font-weight: 500;
	}
	.new-menu-desc {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.reconcile-msg {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
</style>
