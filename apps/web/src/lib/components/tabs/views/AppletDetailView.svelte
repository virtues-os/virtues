<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import AppletSource from '$lib/components/applets/AppletSource.svelte';
	import FaceFrame from '$lib/components/applets/FaceFrame.svelte';
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

	// Editability follows `owner`, because that is genuinely what the server
	// enforces: reconcile owns those rows and would overwrite an edit anyway.
	const isSystem = $derived(action?.owner === 'system');
	const isAgent = $derived(Boolean(action?.agent && action.agent.trim().length > 0));

	// What the user is told, though, follows `origin` — the distinction the
	// list page already learned. Every source fan-out row is owner='system',
	// so keying the EXPLANATION off owner told you the Gmail sync you
	// connected on purpose was an internal system pipeline.
	const managedNote = $derived.by(() => {
		if (!action || !isSystem) return null;
		switch (action.origin) {
			case 'source':
				return 'Part of a source you connected. Its settings come from the connection — disconnect the source to remove it.';
			default:
				return 'Built in. It keeps the box running, so it can be turned off but not deleted — reconcile would recreate it.';
		}
	});

	const triggers = $derived(action?.triggers ?? []);

	// Lifecycle, in words rather than a raw SQL string.
	const lifecycle = $derived.by(() => {
		if (!action) return '';
		if (action.archived_at) {
			return `Finished ${new Date(action.archived_at).toLocaleDateString()}`;
		}
		if (!action.until) return 'Runs for as long as it is on';
		if (action.until.toLowerCase() === 'once') return 'Runs once, then finishes';
		return `Finishes when: ${action.until}`;
	});

	// The ceilings this applet declares, in the same words the gate uses.
	// Read from config rather than a new endpoint — config already ships.
	const limits = $derived.by(() => {
		const l = (action?.config?.limits ?? {}) as Record<string, unknown>;
		const out: string[] = [];
		const money = (k: string, label: string) => {
			const v = typeof l[k] === 'number' ? (l[k] as number) : null;
			if (v !== null) out.push(`${label} $${v.toFixed(2)}`);
		};
		const count = (k: string, label: string) => {
			const v = typeof l[k] === 'number' ? (l[k] as number) : null;
			if (v !== null) out.push(`${label} ${v}`);
		};
		money('max_llm_cost', 'at most'); // per run
		money('max_llm_cost_per_day', 'at most');
		count('max_runs_per_day', 'at most');
		count('max_runs_per_hour', 'at most');
		return out;
	});

	function usd(micros: number): string {
		if (!micros) return '';
		// Sub-cent spend is real and worth showing as more than "$0.00".
		return micros < 10_000 ? `$${(micros / 1_000_000).toFixed(4)}` : `$${(micros / 1_000_000).toFixed(2)}`;
	}

	// What the last 30 runs cost, so the ceiling above has something to mean.
	const recentSpend = $derived(runs.reduce((n, r) => n + (r.cost_micros ?? 0), 0));

	// Milliseconds past the owed slot. Same hour of grace the scheduler and the
	// needs-attention strip use, so the three surfaces never disagree about
	// whether an applet is late.
	const OVERDUE_GRACE_MS = 60 * 60 * 1000;
	const overdueBy = $derived(
		action?.next_due_at
			? Date.now() - new Date(action.next_due_at).getTime() - OVERDUE_GRACE_MS
			: 0
	);

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

	// Put a finished applet back to work. Enabling clears `archived_at`
	// server-side — there is no coherent "enabled and finished" state — so this
	// is one PATCH and then a run, not a separate un-archive verb.
	async function runAgain() {
		if (!action) return;
		saving = true;
		err = null;
		try {
			action = await patchApplet(action.id, { enabled: true });
			await runAction(action.id);
			runs = await listActionRuns(action.id, { limit: 30 });
			action = await getApplet(action.id);
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
					{#if action.description}
						<!-- The intent sentence: what this applet is for, in the user's
						     terms. The page opened with a name and a cron string before
						     this, neither of which says what the thing does. -->
						<p class="intent">{action.description}</p>
					{/if}
					<div class="meta">
						<span>{describeSchedule(action.cron_schedule ?? null)}</span>
						{#if action.next_due_at && action.enabled && !action.archived_at}
							<span class="dot-sep">·</span>
							<!-- The scheduler's own pointer, not a re-derivation from the
							     cron string: an applet that silently stopped firing says
							     so here instead of predicting a run that never comes. -->
							<span class:overdue={overdueBy > 0}>
								{overdueBy > 0
									? `expected ${relativeTime(action.next_due_at)}`
									: `next ${relativeTime(action.next_due_at)}`}
							</span>
						{/if}
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
					{#if action.archived_at}
						<!-- A finished applet has enabled = FALSE, so "Disable" is
						     meaningless and "Run now" would be refused as not-found.
						     One honest affordance instead: put it back to work. -->
						<Button variant="primary" onclick={runAgain} disabled={saving}>
							Run again
						</Button>
					{:else}
						<Button variant="secondary" onclick={toggleEnabled} disabled={saving}>
							{action.enabled ? 'Disable' : 'Enable'}
						</Button>
						<Button variant="primary" onclick={runNow} disabled={saving}>
							Run now
						</Button>
					{/if}
				</div>
			</div>

			{#if err}
				<div class="error-banner">{err}</div>
			{/if}
		</header>

		<!-- The face IS the page when there is one. It was a button to another
		     tab before, which put the applet's own output one click further away
		     than its cron string. -->
		{#if action.has_face}
			<section class="face-block">
				<div class="face-head">
					<h2>What it shows</h2>
					<button type="button" class="open-view" onclick={openView}>
						<Icon icon="ri:external-link-line" width="12" /> Open full page
					</button>
				</div>
				<FaceFrame actionId={action.id} height="460px" />
			</section>
		{/if}

		<div class="body">
			<section class="col main">
				<h2 class="section-head">How it works</h2>

				<label class="field">
					<span class="label">Name</span>
					<input
						type="text"
						bind:value={edit.name}
						disabled={isSystem}
						oninput={markDirty}
					/>
					{#if managedNote}
						<span class="hint">
							<Icon icon="ri:lock-line" width="12" />
							{managedNote}
						</span>
					{/if}
				</label>

				{#if action.description}
					<div class="field">
						<span class="label">What it does</span>
						<p class="readonly-value">{action.description}</p>
						<span class="hint">
							Comes from the applet's manifest. Editing it there and
							reconciling changes it here.
						</span>
					</div>
				{/if}

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
								placeholder="What should this applet do each run?"
							></textarea>
						{:else}
							<div class="pipeline-note">
								<Icon icon="ri:terminal-line" width="14" />
								<span>Subprocess pipeline: <code>{action.function_name}</code></span>
							</div>
						{/if}
						{#if isSystem && isAgent}
							<span class="hint">
								<Icon icon="ri:lock-line" width="12" /> Read-only — this prompt ships with the applet
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

				<!-- Three facts the page never showed, and the reason a person
				     could not tell why an applet had or hadn't run: what wakes
				     it, what it checks once awake, and when it is done. -->
				<div class="field">
					<span class="label">What wakes it</span>
					<div class="chips">
						{#each triggers as t (t)}
							<span class="chip">{t === 'cron' ? 'schedule' : t}</span>
						{/each}
						{#if triggers.length === 0}
							<span class="readonly-value dim">nothing — it never runs on its own</span>
						{/if}
					</div>
				</div>

				{#if action.condition}
					<div class="field">
						<span class="label">Only when</span>
						<code class="readonly-value mono">{action.condition}</code>
						<span class="hint">
							Checked before each run. When it is false the run is skipped, not
							failed.
						</span>
					</div>
				{/if}

				<div class="field">
					<span class="label">Lifecycle</span>
					<p class="readonly-value">{lifecycle}</p>
				</div>

				<div class="field">
					<span class="label">Limits</span>
					{#if limits.length > 0}
						<p class="readonly-value">{limits.join(' · ')}</p>
					{:else}
						<p class="readonly-value dim">No ceilings set.</p>
					{/if}
					{#if recentSpend > 0}
						<span class="hint">
							The last {runs.length} runs cost {usd(recentSpend)}.
						</span>
					{:else if isAgent}
						<span class="hint">The runs below have cost nothing so far.</span>
					{/if}
				</div>

				<label class="field">
					<!-- Not a settings field: this is the applet's own scratchpad,
					     written by it, for its next run. Editing it by hand is
					     allowed and is closer to amending a diary than filling a
					     form, so the label says whose it is. -->
					<span class="label">Notes it keeps</span>
					<textarea
						rows="6"
						bind:value={edit.memory}
						oninput={markDirty}
						placeholder="Empty — this applet has not written itself any notes yet."
					></textarea>
					<span class="hint">
						What this applet wrote down for its own next run. Yours to read, and
						to correct.
					</span>
				</label>

				<div class="save-row">
					{#if !isSystem}
						<Button variant="danger" onclick={openDelete} disabled={saving}>
							Delete applet
						</Button>
					{:else}
						<span class="system-note">
							<Icon icon="ri:lock-line" width="12" />
							{managedNote}
						</span>
					{/if}
					<Button variant="primary" onclick={save} disabled={!isDirty || saving}>
						{saving ? 'Saving…' : 'Save changes'}
					</Button>
				</div>
				<section class="source-block">
					<h3>Source</h3>
					<p class="muted">
						The code this applet runs. Read-only — editing forks it onto this box.
					</p>
					<AppletSource appletId={action.id} />
				</section>
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
													: r.status === 'budget_exceeded'
														? 'warning'
														: 'info'}
									>
										{r.status === 'budget_exceeded' ? 'over budget' : r.status}
									</Badge>
									<span class="run-trigger">{r.trigger}</span>
									{#if r.cost_micros}
										<span class="run-cost" title="What this run spent with the model">
											{usd(r.cost_micros)}
										</span>
									{/if}
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
	.section-head {
		margin: 0;
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--color-foreground-muted);
	}
	.readonly-value {
		margin: 0;
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground);
	}
	.readonly-value.dim {
		color: var(--color-foreground-subtle);
	}
	.readonly-value.mono {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.8125rem;
		padding: 0.5rem 0.625rem;
		border-radius: 6px;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border-subtle);
		display: block;
		overflow-x: auto;
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
	}
	.chip {
		padding: 0.1rem 0.5rem;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		background: var(--color-surface-elevated);
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}
	.face-block {
		padding: 1.25rem 2rem 0;
		max-width: 1400px;
		width: 100%;
		margin: 0 auto;
	}
	.face-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		margin-bottom: 0.5rem;
	}
	.face-head h2 {
		margin: 0;
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--color-foreground-muted);
	}
	.intent {
		margin: 0 0 0.375rem;
		max-width: 68ch;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
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
	/* Lifecycle is a neutral fact — "runs forever" is not a warning. This was
	   tinted `--color-warning`, which made every applet's normal state read as
	   a problem and left nothing distinct for the state that IS one. */
	.muted-inline {
		color: inherit;
	}
	/* The slot passed and nothing ran. The one thing in this row that earns a
	   colour, now that it is the only one taking it. */
	.overdue {
		color: var(--color-warning);
		font-weight: 500;
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
	.run-cost {
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
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

	.source-block {
		margin-top: 1.5rem;
		padding-top: 1.25rem;
		border-top: 1px solid var(--color-border, #e5e7eb);
	}
	.source-block h3 {
		margin: 0 0 0.25rem;
		font-size: 0.875rem;
		font-weight: 600;
	}
	.source-block .muted {
		margin: 0 0 0.75rem;
		font-size: 0.75rem;
		font-style: normal;
	}
</style>
