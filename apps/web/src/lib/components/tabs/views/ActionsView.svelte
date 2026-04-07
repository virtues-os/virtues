<script lang="ts">
	import { onMount } from 'svelte';
	import { listActions, type Action } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';

	let actions = $state<Action[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(async () => {
		try {
			actions = await listActions();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load actions';
		} finally {
			loading = false;
		}
	});

	function formatCron(cron: string | null): string {
		if (!cron) return 'Manual';
		// Simple human-readable for common patterns
		if (cron === '0 * * * * *') return 'Every minute';
		if (cron === '0 0 * * * *') return 'Hourly';
		if (cron === '0 */15 * * * *') return 'Every 15 min';
		if (cron === '0 */30 * * * *') return 'Every 30 min';
		if (cron.match(/^0 0 \d+ \* \* \*$/)) return `Daily at ${cron.split(' ')[2]}:00 UTC`;
		if (cron.match(/^0 0 \*\/\d+ \* \* \*$/)) {
			const hours = cron.split(' ')[2].replace('*/', '');
			return `Every ${hours}h`;
		}
		return cron;
	}

	function formatTime(ts: string | null): string {
		if (!ts) return '-';
		try {
			const d = new Date(ts);
			return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
		} catch {
			return ts;
		}
	}

	function typeLabel(action: Action): string {
		if (action.action_type === 'agent') return 'Agent';
		if (action.action_type === 'sync') return 'Sync';
		if (action.action_type === 'system') return 'System';
		return action.action_type;
	}
</script>

<div class="actions-view">
	<header class="actions-header">
		<h1>Actions</h1>
		<p class="subtitle">Scheduled tasks, syncs, and automations</p>
	</header>

	{#if loading}
		<p class="loading">Loading actions...</p>
	{:else if error}
		<p class="error">{error}</p>
	{:else if actions.length === 0}
		<p class="empty">No actions configured.</p>
	{:else}
		<div class="table-wrapper">
			<table>
				<thead>
					<tr>
						<th>Name</th>
						<th>Type</th>
						<th>Schedule</th>
						<th>Enabled</th>
						<th>Owner</th>
						<th>Last Run</th>
						<th>Status</th>
					</tr>
				</thead>
				<tbody>
					{#each actions as action}
						<tr class:disabled={!action.enabled}>
							<td class="name-cell">
								<span class="action-name">{action.name}</span>
								<span class="action-id">{action.id}</span>
							</td>
							<td><span class="badge type-{action.action_type}">{typeLabel(action)}</span></td>
							<td class="mono">{formatCron(action.cron_schedule)}</td>
							<td>
								{#if action.enabled}
									<Icon name="ri:checkbox-circle-fill" size={16} />
								{:else}
									<Icon name="ri:close-circle-line" size={16} />
								{/if}
							</td>
							<td>
								<span class="badge {action.owner}">{action.owner}</span>
							</td>
							<td class="mono">{formatTime(action.last_run?.started_at ?? null)}</td>
							<td>
								{#if action.last_run}
									<span class="badge status-{action.last_run.status}">{action.last_run.status}</span>
									{#if action.last_run.error}
										<span class="error-hint" title={action.last_run.error}>
											<Icon name="ri:error-warning-line" size={14} />
										</span>
									{/if}
								{:else}
									<span class="no-runs">-</span>
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>

<style>
	.actions-view {
		padding: 2rem;
		max-width: 1200px;
		margin: 0 auto;
	}

	.actions-header h1 {
		font-size: 1.5rem;
		font-weight: 600;
		margin: 0 0 0.25rem;
	}

	.subtitle {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		margin: 0 0 1.5rem;
	}

	.loading, .error, .empty {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		font-style: italic;
	}

	.error { color: var(--color-danger, #e53e3e); }

	.table-wrapper {
		overflow-x: auto;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8125rem;
	}

	th {
		text-align: left;
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
		font-weight: 500;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-foreground-subtle);
	}

	td {
		padding: 0.625rem 0.75rem;
		border-bottom: 1px solid var(--color-border-subtle, var(--color-border));
		vertical-align: middle;
	}

	tr.disabled td {
		opacity: 0.5;
	}

	.name-cell {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.action-name {
		font-weight: 500;
	}

	.action-id {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		font-family: var(--font-mono, monospace);
	}

	.mono {
		font-family: var(--font-mono, monospace);
		font-size: 0.75rem;
	}

	.badge {
		display: inline-block;
		padding: 0.125rem 0.5rem;
		border-radius: 4px;
		font-size: 0.6875rem;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}

	.type-agent { background: var(--color-primary-subtle, #e0e7ff); color: var(--color-primary, #4f46e5); }
	.type-sync { background: #e0f2fe; color: #0369a1; }
	.type-system { background: #fef3c7; color: #92400e; }
	.system { background: var(--color-surface-elevated, #f3f4f6); color: var(--color-foreground-subtle); }

	.status-success { background: #d1fae5; color: #065f46; }
	.status-error { background: #fee2e2; color: #991b1b; }
	.status-running { background: #dbeafe; color: #1e40af; }
	.status-skipped { background: #f3f4f6; color: #6b7280; }
	.status-cancelled { background: #fef3c7; color: #92400e; }

	.error-hint {
		margin-left: 0.25rem;
		color: var(--color-danger, #e53e3e);
		cursor: help;
	}

	.no-runs {
		color: var(--color-foreground-subtle);
	}
</style>
