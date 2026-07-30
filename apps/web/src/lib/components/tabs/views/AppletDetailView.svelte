<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import Button from '$lib/components/Button.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { routeToEntityId } from '$lib/tabs/types';
	import type { Tab } from '$lib/tabs/types';
	import {
		getApplet,
		listActionRuns,
		patchApplet,
		deleteAction,
		getAppletData,
		runAction,
		type Applet,
		type AppletRun,
		type AppletData,
		type PatchAppletBody
	} from '$lib/api/client';
	import { relativeTime, describeSchedule } from '$lib/applets/palette';

	let { tab }: { tab: Tab; active: boolean } = $props();

	const actionId = $derived(routeToEntityId(tab.route));

	let action = $state<Applet | null>(null);
	let runs = $state<AppletRun[]>([]);
	let loading = $state(false);
	let saving = $state(false);
	let err = $state<string | null>(null);

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
			const [a, rs] = await Promise.all([getApplet(id), listActionRuns(id, { limit: 30 })]);
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
			const patch: PatchAppletBody = {};
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
			const updated = await patchApplet(action.id, patch);
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
			action = await patchApplet(action.id, { enabled: !action.enabled });
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}

	function openView() {
		if (!action) return;
		windowShellStore.openTabFromRoute(`/applet/${action.id}/view`);
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

	// Delete confirm. Loads the applet's owned tables so the user can decide
	// whether to also drop its data (default: keep — data outlives the applet).
	let deleteOpen = $state(false);
	let deleteData = $state<AppletData | null>(null);
	let dropData = $state(false);
	let deleting = $state(false);

	async function openDelete() {
		if (!action) return;
		deleteOpen = true;
		dropData = false;
		deleteData = null;
		deleteData = await getAppletData(action.id);
	}

	async function doDelete() {
		if (!action) return;
		deleting = true;
		err = null;
		try {
			await deleteAction(action.id, dropData);
			deleteOpen = false;
			windowShellStore.closeTab(tab.id);
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			deleting = false;
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
			{#if action.has_face}
				<div class="meta view-link">
					<button type="button" class="open-view" onclick={openView}>
						<Icon icon="ri:layout-2-line" width="13" /> Open view
					</button>
				</div>
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

				<!-- A pure View (a face with no agent) has no server-side run and
				     no prompt — don't show an empty agent editor for it. -->
				{#if isAgent || !action.has_face}
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
				{/if}

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
						<Button variant="danger" onclick={openDelete} disabled={saving}>
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

	{/if}
</div>

{#if action}
	<Modal open={deleteOpen} onClose={() => (deleteOpen = false)} title="Delete applet" width="sm">
		<div class="del">
			<p>
				Delete <strong>{action.name}</strong>? This removes the applet and can't be undone.
			</p>
			{#if deleteData && deleteData.tables.length > 0}
				<label class="drop-opt">
					<input type="checkbox" bind:checked={dropData} />
					<span>
						Also permanently delete its data
						<span class="dim"
							>({deleteData.tables.length}
							{deleteData.tables.length === 1 ? 'table' : 'tables'} in
							<code>{deleteData.schema}</code>)</span
						>
					</span>
				</label>
				<ul class="tbl-list">
					{#each deleteData.tables as t (t)}
						<li><code>{t}</code></li>
					{/each}
				</ul>
				{#if !dropData}
					<p class="keep-note dim">Its data will be kept and can outlive the applet.</p>
				{/if}
			{/if}
		</div>
		{#snippet footer()}
			<Button variant="ghost" onclick={() => (deleteOpen = false)} disabled={deleting}>Cancel</Button>
			<Button variant="danger" onclick={doDelete} disabled={deleting}>
				{deleting ? 'Deleting…' : dropData ? 'Delete applet + data' : 'Delete applet'}
			</Button>
		{/snippet}
	</Modal>
{/if}

<style>
	.del {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		font-size: 0.875rem;
	}
	.del p {
		margin: 0;
	}
	.drop-opt {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		cursor: pointer;
	}
	.drop-opt input {
		margin-top: 0.15rem;
	}
	.tbl-list {
		margin: 0;
		padding-left: 1.5rem;
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		max-height: 8rem;
		overflow-y: auto;
	}
	.del code {
		font-size: 0.8125rem;
	}
	.keep-note {
		margin: 0;
		font-size: 0.8125rem;
	}
	.del .dim {
		color: var(--color-foreground-subtle);
	}
	.view-link {
		margin-top: 0.5rem;
	}
	.open-view {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		padding: 0.25rem 0.6rem;
		font-size: 0.8125rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		cursor: pointer;
	}
	.open-view:hover { border-color: var(--color-foreground-subtle); }

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
