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
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { describeSchedule, relativeTime } from '$lib/actions/palette';
	import ActionCard from './ActionCard.svelte';
	import GitImportModal from './GitImportModal.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';

	let actions = $state<Action[]>([]);
	let pulseByAction = $state<Record<string, ActionRun[]>>({});
	let lastSuccessByAction = $state<Record<string, ActionRun | null>>({});
	let loading = $state(true);
	let err = $state<string | null>(null);
	let newMenuOpen = $state(false);
	let gitImportOpen = $state(false);
	let reconciling = $state(false);
	let reconcileMsg = $state<string | null>(null);
	// Built-in (system) applets are plumbing — inspectable on demand, hidden
	// by default so they don't crowd out yours. A filter, not a wall.
	let showSystem = $state(false);

	// Archived applets (lifecycle complete) are hidden: the list holds
	// living things. Their run history stays reachable from chat/detail.
	const living = $derived(actions.filter((a) => !a.archived_at));
	const systemCount = $derived(living.filter((a) => a.owner === 'system').length);
	const visible = $derived(showSystem ? living : living.filter((a) => a.owner !== 'system'));

	// Needs-attention strip: enabled applets whose last run errored.
	// (Expected-but-didn't-run and credential-expired join when the slot
	// bookkeeping and credential surfacing land.)
	const needsAttention = $derived(
		living.filter((a) => a.enabled && a.last_run?.status === 'error')
	);

	function startChatFlow() {
		newMenuOpen = false;
		windowShellStore.openTabFromRoute('/chat', { forceNew: true });
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
			reconcileMsg = `${out.upserted} upserted`;
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

	function openView(a: Action) {
		windowShellStore.openTabFromRoute(`/applet/${a.id}/view`);
	}

	function openDetail(a: Action) {
		windowShellStore.openAside({
			type: 'action',
			label: a.name,
			route: `/applet/${a.id}`,
			icon: 'ri:flashlight-line'
		});
	}

	// Default open: an applet with a face goes straight to its full-page view;
	// otherwise to its settings/detail.
	function openCard(a: Action) {
		if (a.has_face) openView(a);
		else openDetail(a);
	}

	// Right-click: pick view (if it has one) or settings explicitly.
	function rowContextMenu(a: Action, e: MouseEvent) {
		e.preventDefault();
		const items = [];
		if (a.has_face) {
			items.push({
				id: 'view',
				label: 'Open view',
				icon: 'ri:layout-2-line',
				action: () => openView(a)
			});
		}
		items.push({
			id: 'detail',
			label: 'Settings & runs',
			icon: 'ri:settings-3-line',
			action: () => openDetail(a)
		});
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	function lastRunStatus(action: Action): string {
		const lr = action.last_run;
		if (!lr) return '—';
		return lr.status ?? '—';
	}

	function lifecycleLabel(a: Action): string {
		if (!a.until) return 'forever';
		return a.until.toLowerCase() === 'once' ? 'once' : 'until';
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
			key: 'until',
			label: 'Lifecycle',
			format: 'badge',
			getValue: (a) => lifecycleLabel(a),
			badgeColors: {
				forever: 'badge-muted',
				once: 'badge-info',
				until: 'badge-info'
			}
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
			id: 'owner',
			kind: 'multi',
			label: 'Owner',
			options: [
				{ value: 'ai', label: 'AI-authored' },
				{ value: 'user', label: 'User' },
				{ value: 'system', label: 'Built-in' }
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
			<h2>Applets</h2>
			<p class="subtitle">
				Things that run for you. Ask in chat — "remind me on the 25th,"
				"a dashboard of my heart rate," "write my examen each morning" —
				and it becomes an applet: scheduled, triggered, or always on.
			</p>
		</div>
		<div class="header-actions">
			{#if reconcileMsg}
				<span class="reconcile-msg">{reconcileMsg}</span>
			{/if}
			<button
				type="button"
				class="show-system-btn"
				class:active={showSystem}
				onclick={() => (showSystem = !showSystem)}
				title="Built-in applets keep the box running (syncs, indexing). Inspectable, just not in the way."
			>
				{showSystem ? 'Hide' : 'Show'} built-in ({systemCount})
			</button>
			<button
				type="button"
				class="reconcile-btn"
				disabled={reconciling}
				onclick={reconcile}
				title="Re-read applet manifests from disk and apply changes"
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
								<div class="new-menu-desc">Import applets from a repo</div>
							</div>
						</button>
					</div>
				{/snippet}
			</Popover>
		</div>
	</header>

	{#if needsAttention.length > 0}
		<div class="attention-strip" role="alert">
			<Icon icon="ri:error-warning-line" width="16" />
			<span class="attention-label">
				{needsAttention.length === 1
					? '1 applet needs attention'
					: `${needsAttention.length} applets need attention`}
			</span>
			<div class="attention-items">
				{#each needsAttention as a (a.id)}
					<button type="button" class="attention-item" onclick={() => openCard(a)}>
						{a.name}
						<span class="attention-why">last run failed</span>
					</button>
				{/each}
			</div>
		</div>
	{/if}

	<UniversalDataGrid
		items={visible}
		{columns}
		{filters}
		entityType="actions"
		defaultViewMode="table"
		gridMinWidth="340px"
		{loading}
		error={err}
		emptyIcon="ri:flashlight-line"
		emptyMessage="No applets yet — ask for one in chat."
		searchPlaceholder="Search applets…"
		pageSize={50}
		onItemClick={openCard}
		onItemContextMenu={rowContextMenu}
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

	.show-system-btn {
		padding: 0.375rem 0.625rem;
		font-size: 0.8125rem;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-subtle, #6b7280);
		cursor: pointer;
	}
	.show-system-btn:hover,
	.show-system-btn.active {
		color: var(--color-foreground, #111827);
		background: var(--color-surface-elevated, #f3f4f6);
	}

	/* Theme tokens only — --color-error/-subtle are defined per theme
	   (light and dark); no hardcoded fallbacks that break dark themes. */
	.attention-strip {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		flex-wrap: wrap;
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--color-error-subtle);
		border-radius: 8px;
		background: var(--color-error-subtle);
		color: var(--color-error);
		font-size: 0.8125rem;
	}
	.attention-label {
		font-weight: 500;
	}
	.attention-items {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		flex-wrap: wrap;
	}
	.attention-item {
		display: inline-flex;
		align-items: baseline;
		gap: 0.375rem;
		padding: 0.125rem 0.5rem;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		background: var(--color-surface);
		color: var(--color-error);
		font-size: 0.75rem;
		cursor: pointer;
	}
	.attention-item:hover {
		background: var(--color-surface-elevated);
	}
	.attention-why {
		color: var(--color-foreground-subtle);
		font-size: 0.6875rem;
	}
</style>
