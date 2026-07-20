<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import Button from '$lib/components/Button.svelte';
	import LogsPanel from '$lib/components/actions/LogsPanel.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { routeToEntityId } from '$lib/tabs/types';
	import type { Tab } from '$lib/tabs/types';
	import {
		getAction,
		listActionRuns,
		patchAction,
		deleteAction,
		runAction,
		listSystemApps,
		type Action,
		type ActionRun,
		type PatchActionBody,
		type RunningApp
	} from '$lib/api/client';
	import { relativeTime, describeSchedule } from '$lib/actions/palette';

	let { tab }: { tab: Tab; active: boolean } = $props();

	const actionId = $derived(routeToEntityId(tab.route));

	let action = $state<Action | null>(null);
	let runs = $state<ActionRun[]>([]);
	let loading = $state(false);
	let saving = $state(false);
	let err = $state<string | null>(null);
	let appState = $state<RunningApp | null>(null);

	$effect(() => {
		if (!action?.supervise || !action?.id) return;
		const id = action.id;
		const fetch = async () => {
			try {
				const apps = await listSystemApps();
				appState = apps.find((a) => a.action_id === id) ?? null;
			} catch {
				// non-fatal — supervisor probe is optional context
			}
		};
		void fetch();
		const t = setInterval(fetch, 2000);
		return () => clearInterval(t);
	});

	function appStatusVariant(s: RunningApp['status']): string {
		switch (s) {
			case 'Running': return 'badge-success';
			case 'Starting': return 'badge-info';
			case 'Backoff': return 'badge-warning';
			case 'Crashed': return 'badge-error';
			case 'Stopping': return 'badge-muted';
		}
	}

	let edit = $state<{ name: string; agent: string; cron_schedule: string; memory: string }>({
		name: '',
		agent: '',
		cron_schedule: '',
		memory: ''
	});
	let isDirty = $state(false);

	$effect(() => {
		if (actionId) void load(actionId);
	});

	async function load(id: string) {
		loading = true;
		err = null;
		try {
			const [a, rs] = await Promise.all([getAction(id), listActionRuns(id, { limit: 30 })]);
			action = a;
			runs = rs;
			edit = {
				name: a.name,
				agent: a.agent ?? '',
				cron_schedule: a.cron_schedule ?? '',
				memory: a.memory ?? ''
			};
			isDirty = false;
			windowShellStore.updateTab(tab.id, { label: a.name });
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	const isSystem = $derived(action?.owner === 'system');
	const isAgent = $derived(Boolean(action?.agent && action.agent.trim().length > 0));

	function markDirty() {
		isDirty = true;
	}

	async function save() {
		if (!action) return;
		saving = true;
		err = null;
		try {
			const patch: PatchActionBody = {};
			if (!isSystem && edit.name !== action.name) patch.name = edit.name;
			if (!isSystem && edit.agent !== (action.agent ?? '')) {
				patch.agent = edit.agent.trim() ? edit.agent : null;
			}
			if (edit.cron_schedule !== (action.cron_schedule ?? '')) {
				patch.cron_schedule = edit.cron_schedule.trim() ? edit.cron_schedule : null;
			}
			if (edit.memory !== (action.memory ?? '')) {
				patch.memory = edit.memory.trim() ? edit.memory : null;
			}
			if (Object.keys(patch).length === 0) {
				isDirty = false;
				return;
			}
			const updated = await patchAction(action.id, patch);
			action = updated;
			edit = {
				name: updated.name,
				agent: updated.agent ?? '',
				cron_schedule: updated.cron_schedule ?? '',
				memory: updated.memory ?? ''
			};
			isDirty = false;
			windowShellStore.updateTab(tab.id, { label: updated.name });
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}

	async function toggleEnabled() {
		if (!action) return;
		saving = true;
		err = null;
		try {
			action = await patchAction(action.id, { enabled: !action.enabled });
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}

	async function runNow() {
		if (!action) return;
		saving = true;
		err = null;
		try {
			await runAction(action.id);
			runs = await listActionRuns(action.id, { limit: 30 });
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}

	async function confirmDelete() {
		if (!action) return;
		if (!confirm(`Delete "${action.name}"? This can't be undone.`)) return;
		saving = true;
		err = null;
		try {
			await deleteAction(action.id);
			windowShellStore.closeTab(tab.id);
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}
</script>

<div class="detail">
	{#if loading && !action}
		<div class="state">
			<p class="muted">Loading…</p>
		</div>
	{:else if err && !action}
		<div class="state">
			<p class="error-msg">{err}</p>
		</div>
	{:else if action}
		<header class="hero">
			<div class="hero-top">
				<div class="title-block">
					<h1 class="title">{action.name}</h1>
					<div class="meta">
						<span>{describeSchedule(action.cron_schedule ?? null)}</span>
						<span class="dot-sep">·</span>
						<span class="muted-inline">
							{#if action.archived_at}
								archived {new Date(action.archived_at).toLocaleDateString()}
							{:else if !action.until}
								runs forever
							{:else if action.until.toLowerCase() === 'once'}
								runs once, then archives
							{:else}
								runs until: {action.until}
							{/if}
						</span>
						{#if !action.enabled && !action.archived_at}
							<span class="dot-sep">·</span>
							<span class="muted-inline">disabled</span>
						{/if}
					</div>
				</div>
				<div class="hero-actions">
					<Button variant="secondary" onclick={toggleEnabled} disabled={saving}>
						{action.enabled ? 'Disable' : 'Enable'}
					</Button>
					<Button variant="primary" onclick={runNow} disabled={saving}>
						Run now
					</Button>
				</div>
			</div>

			{#if err}
				<div class="error-banner">{err}</div>
			{/if}
		</header>

		<div class="body">
			<section class="col main">
				<label class="field">
					<span class="label">Name</span>
					<input
						type="text"
						bind:value={edit.name}
						disabled={isSystem}
						oninput={markDirty}
					/>
					{#if isSystem}
						<span class="hint">
							<Icon icon="ri:lock-line" width="12" /> Managed by templates.toml
						</span>
					{/if}
				</label>

				<label class="field">
					<span class="label">Agent prompt</span>
					{#if isAgent || !isSystem}
						<textarea
							rows="10"
							bind:value={edit.agent}
							disabled={isSystem}
							oninput={markDirty}
							placeholder="What should this action do each run?"
						></textarea>
					{:else}
						<div class="pipeline-note">
							<Icon icon="ri:terminal-line" width="14" />
							<span>Subprocess pipeline: <code>{action.function_name}</code></span>
						</div>
					{/if}
					{#if isSystem && isAgent}
						<span class="hint">
							<Icon icon="ri:lock-line" width="12" /> System prompt — read only
						</span>
					{/if}
				</label>

				<label class="field">
					<span class="label">Schedule</span>
					<input
						type="text"
						bind:value={edit.cron_schedule}
						placeholder="0 0 * * * *  (6-field cron, empty = on-demand)"
						oninput={markDirty}
					/>
					<span class="hint">{describeSchedule(edit.cron_schedule || null)}</span>
				</label>

				<label class="field">
					<span class="label">Memory</span>
					<textarea
						rows="6"
						bind:value={edit.memory}
						oninput={markDirty}
						placeholder="Persistent markdown scratchpad, carried across runs"
					></textarea>
				</label>

				<div class="save-row">
					{#if !isSystem}
						<Button variant="danger" onclick={confirmDelete} disabled={saving}>
							Delete action
						</Button>
					{:else}
						<span class="system-note">
							<Icon icon="ri:lock-line" width="12" />
							System action — managed automatically. Disable it to stop it running; it can't be deleted (reconcile would recreate it).
						</span>
					{/if}
					<Button variant="primary" onclick={save} disabled={!isDirty || saving}>
						{saving ? 'Saving…' : 'Save changes'}
					</Button>
				</div>
			</section>

			<aside class="col runs">
				<h3>Recent runs</h3>
				{#if runs.length === 0}
					<p class="muted">No runs yet.</p>
				{:else}
					<ul class="runs-list">
						{#each runs as r}
							<li class="run-item" data-status={r.status}>
								<div class="run-top">
									<Badge
										variant={r.status === 'success'
											? 'success'
											: r.status === 'error'
												? 'error'
												: r.status === 'skipped'
													? 'muted'
													: 'info'}
									>
										{r.status}
									</Badge>
									<span class="run-trigger">{r.trigger}</span>
									<span class="run-time">{relativeTime(r.started_at)}</span>
								</div>
								{#if r.result_summary}
									<p class="run-summary">{r.result_summary}</p>
								{/if}
								{#if r.error}
									<p class="run-error">{r.error}</p>
								{/if}
							</li>
						{/each}
					</ul>
				{/if}
			</aside>
		</div>

		<!-- Logs panel — only meaningful for `app`-runtime actions, which
		     have long-lived stdout/stderr captured in the supervisor's per-app
		     ring buffer. `function` runs surface their output via runs above;
		     `view` runtimes have no server-side execution. -->
		{#if action.supervise}
			<div class="app-meta">
				{#if appState}
					<span class="meta-badge {appStatusVariant(appState.status)}">{appState.status}</span>
					<span class="meta-item"><span class="meta-label">port</span> <span class="mono">{appState.port}</span></span>
					<span class="meta-item"><span class="meta-label">pid</span> <span class="mono">{appState.pid ?? '—'}</span></span>
					<span class="meta-item"><span class="meta-label">restarts</span> <span class="mono">{appState.restart_count}</span></span>
				{:else}
					<span class="meta-item dim">app not currently supervised</span>
				{/if}
			</div>
			<div class="logs-section">
				<LogsPanel actionId={action.id} />
			</div>
		{/if}
	{/if}
</div>

<style>
	.detail {
		display: flex;
		flex-direction: column;
		height: 100%;
		overflow-y: auto;
		background: var(--color-surface, #fff);
	}

	.state {
		padding: 3rem 1.5rem;
		text-align: center;
	}

	.hero {
		padding: 1.25rem 2rem 1rem;
		border-bottom: 1px solid var(--color-border, #e5e7eb);
		background: var(--color-surface, #fff);
		position: sticky;
		top: 0;
		z-index: 1;
	}
	.hero-top {
		display: flex;
		align-items: flex-start;
		gap: 1rem;
	}
	.title-block {
		flex: 1;
		min-width: 0;
	}
	.title {
		margin: 0 0 0.25rem;
		font-size: 1.1875rem;
		font-weight: 600;
		line-height: 1.25;
	}
	.meta {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.dot-sep {
		opacity: 0.5;
	}
	.muted-inline {
		color: var(--color-warning);
	}
	.hero-actions {
		display: flex;
		gap: 0.5rem;
		flex-shrink: 0;
	}
	.error-banner {
		margin-top: 0.75rem;
		padding: 0.5rem 0.75rem;
		border-radius: 6px;
		background: var(--color-error-subtle);
		color: var(--color-error);
		font-size: 0.8125rem;
	}

	.body {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 380px);
		gap: 1.5rem;
		padding: 1.5rem 2rem 2rem;
		max-width: 1400px;
		width: 100%;
		margin: 0 auto;
	}
	@media (max-width: 900px) {
		.body {
			grid-template-columns: 1fr;
		}
	}

	.logs-section {
		padding: 0 2rem 2rem;
		max-width: 1400px;
		width: 100%;
		margin: 0 auto;
	}
	.app-meta {
		display: flex;
		align-items: center;
		gap: 0.875rem;
		padding: 0 2rem 0.75rem;
		max-width: 1400px;
		width: 100%;
		margin: 0 auto;
		font-size: 0.75rem;
		flex-wrap: wrap;
	}
	.meta-badge {
		display: inline-block;
		padding: 0.0625rem 0.375rem;
		border-radius: 4px;
		font-size: 0.6875rem;
		font-weight: 500;
	}
	/* meta-badge inherits the global .badge-* color from app.css */
	.meta-item {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
	}
	.meta-label {
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.meta-item .mono {
		font-family: var(--font-mono, ui-monospace, monospace);
	}
	.meta-item.dim {
		color: var(--color-foreground-subtle, #9ca3af);
		font-style: italic;
	}

	.col {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		min-width: 0;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.label {
		font-size: 0.75rem;
		font-weight: 500;
		color: var(--color-foreground-muted, #6b7280);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.field input,
	.field textarea {
		font: inherit;
		font-size: 0.875rem;
		padding: 0.5rem 0.625rem;
		border-radius: 6px;
		border: 1px solid var(--color-border, #e5e7eb);
		background: var(--color-surface, #fff);
		color: var(--color-foreground, inherit);
		resize: vertical;
	}
	.field textarea {
		font-family: var(--font-sans, inherit);
		line-height: 1.5;
	}
	.field input:disabled,
	.field textarea:disabled {
		background: var(--color-surface-elevated, #f9fafb);
		opacity: 0.7;
		cursor: not-allowed;
	}
	.hint {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle, #9ca3af);
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
	}
	.pipeline-note {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
		background: var(--color-surface-elevated, #f3f4f6);
		border-radius: 6px;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.pipeline-note code {
		font-family: var(--font-mono, monospace);
		font-size: 0.75rem;
	}

	.save-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding-top: 0.5rem;
		border-top: 1px solid var(--color-border-subtle, #f3f4f6);
		margin-top: 0.5rem;
	}
	.system-note {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		max-width: 60ch;
		font-size: 0.6875rem;
		line-height: 1.4;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.runs {
		border-left: 1px solid var(--color-border-subtle, #f3f4f6);
		padding-left: 1.5rem;
	}
	@media (max-width: 900px) {
		.runs {
			border-left: none;
			border-top: 1px solid var(--color-border-subtle, #f3f4f6);
			padding-left: 0;
			padding-top: 1rem;
		}
	}
	.runs h3 {
		font-size: 0.8125rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--color-foreground-muted, #6b7280);
		margin: 0 0 0.75rem;
	}
	.runs-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.run-item {
		padding: 0.625rem 0.75rem;
		border-radius: 6px;
		background: var(--color-surface-elevated, #f9fafb);
		border: 1px solid var(--color-border-subtle, #f3f4f6);
	}
	.run-top {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.75rem;
	}
	.run-trigger {
		font-family: var(--font-mono, monospace);
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.run-time {
		margin-left: auto;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.run-summary {
		margin: 0.375rem 0 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #4b5563);
	}
	.run-error {
		margin: 0.375rem 0 0;
		font-size: 0.75rem;
		color: color-mix(in srgb, var(--color-error) 75%, #000);
		font-family: var(--font-mono, monospace);
	}

	.muted {
		color: var(--color-foreground-subtle, #9ca3af);
		font-style: italic;
	}
	.error-msg {
		color: color-mix(in srgb, var(--color-error) 75%, #000);
	}
</style>
