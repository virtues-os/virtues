<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import Card from '$lib/components/Card.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import DayDot from './DayDot.svelte';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import {
		patchApplet,
		deleteApplet,
		getAppletData,
		type Applet,
		type AppletData
	} from '$lib/api/client';
	import { describeSchedule, limitsOf, relativeTime } from '$lib/applets/palette';
	import { formatMicrosPrecise } from '$lib/utils/currency';
	import {
		appletDays,
		countRuns,
		DAY_LABEL,
		lastDays,
		recentSummary,
		type DayIndex
	} from '$lib/applets/days';

	/**
	 * An applet's Info: is it working, when does it run, what does it cost,
	 * and the switch that turns it off. Everything else is one step further,
	 * under Technical details.
	 */
	let {
		action = $bindable(),
		index,
		daysErr,
		onHistory,
		onTechnical,
		onDeleted
	}: {
		action: Applet;
		index: DayIndex;
		daysErr: string | null;
		onHistory: () => void;
		onTechnical: () => void;
		onDeleted: () => void;
	} = $props();

	let saving = $state(false);
	let err = $state<string | null>(null);

	const week = lastDays(7);
	const states = $derived(appletDays(action, index, week));
	const summary = $derived(recentSummary(states, countRuns(index, action.id, week)));
	const isSystem = $derived(action.owner === 'system');

	// Same grace the scheduler and the table use, so they agree on "late".
	const OVERDUE_GRACE_MS = 60 * 60 * 1000;
	const nextRun = $derived.by(() => {
		if (!action.next_due_at || !action.enabled || action.archived_at) return null;
		const late = Date.now() - new Date(action.next_due_at).getTime() > OVERDUE_GRACE_MS;
		// The scheduler's own pointer, not a re-derivation from the cron
		// string: an applet that stopped firing says so instead of predicting
		// a run that never comes.
		return late
			? `Was due ${relativeTime(action.next_due_at)}`
			: `Next ${relativeTime(action.next_due_at)}`;
	});

	const limits = $derived(limitsOf(action.config));
	const spend = $derived.by(() => {
		// null means the box couldn't read it, which is not the same as free.
		if (action.spend_week_micros === null) return "Your server couldn't read this week's cost";
		if (action.spend_week_micros === 0) return 'Nothing in the last 7 days';
		// Below what four decimals can show, "$0.00" would read as free.
		if (action.spend_week_micros < 100) return 'Less than a cent in the last 7 days';
		return `${formatMicrosPrecise(action.spend_week_micros)} in the last 7 days`;
	});

	async function setEnabled(enabled: boolean) {
		saving = true;
		err = null;
		try {
			// Turning a finished applet on clears `archived_at` on the server:
			// there is no "on and finished" state.
			action = await patchApplet(action.id, { enabled });
			void appletsStore.load();
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
		deleteOpen = true;
		dropData = false;
		deleteData = null;
		deleteData = await getAppletData(action.id);
	}

	async function doDelete() {
		deleting = true;
		err = null;
		try {
			await deleteApplet(action.id, dropData);
			deleteOpen = false;
			void appletsStore.load();
			onDeleted();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			deleting = false;
		}
	}
</script>

<div class="info">
	{#if action.description}
		<p class="desc">{action.description}</p>
	{/if}

	<section>
		<h3 class="group-head">Run history</h3>
		<Card list>
			<button type="button" class="row history-row" onclick={onHistory}>
				{#if daysErr}
					<span class="row-label">Your server couldn't read the last 7 days</span>
				{:else}
					<span class="dots">
						{#each states as state, i (week[i])}
							<DayDot {state} size="md" title={`${week[i]}: ${DAY_LABEL[state]}`} />
						{/each}
					</span>
					<span class="row-label">{summary}</span>
				{/if}
				<Icon icon="ri:arrow-right-s-line" width="16" />
			</button>
		</Card>
	</section>

	<Card list>
		<div class="row">
			<span class="row-label">On</span>
			{#if action.archived_at}
				<span class="row-value">Finished {new Date(action.archived_at).toLocaleDateString()}</span>
				<Button variant="secondary" size="sm" onclick={() => setEnabled(true)} disabled={saving}>
					Turn back on
				</Button>
			{:else}
				<button
					type="button"
					class="switch"
					class:on={action.enabled}
					role="switch"
					aria-checked={action.enabled}
					aria-label="Run this applet"
					disabled={saving}
					onclick={() => setEnabled(!action.enabled)}
				></button>
			{/if}
		</div>
		<div class="row">
			<span class="row-label">Runs</span>
			<span class="row-value">
				{describeSchedule(action.schedule)}{#if nextRun}<span class="sub">{nextRun}</span>{/if}
			</span>
		</div>
		<div class="row">
			<span class="row-label">Cost</span>
			<span class="row-value">
				{spend}{#if limits.length}<span class="sub">Limit {limits.join(', ')}</span>{/if}
			</span>
		</div>
	</Card>

	<Card list>
		<button type="button" class="row" onclick={onTechnical}>
			<span class="row-label">Technical details</span>
			<Icon icon="ri:arrow-right-s-line" width="16" />
		</button>
	</Card>

	{#if err}
		<p class="error-msg">{err}</p>
	{/if}

	{#if !isSystem}
		<div class="delete-row">
			<Button variant="danger" size="sm" onclick={openDelete} disabled={saving}>Delete applet</Button>
		</div>
	{/if}
</div>

<Modal open={deleteOpen} onClose={() => (deleteOpen = false)} title="Delete applet" width="sm">
	<div class="del">
		<p>Delete <strong>{action.name}</strong>? This removes the applet and can't be undone.</p>
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
				<p class="dim">Your server keeps its data, which can outlive the applet.</p>
			{/if}
		{/if}
	</div>
	{#snippet footer()}
		<Button variant="ghost" onclick={() => (deleteOpen = false)} disabled={deleting}>Cancel</Button>
		<Button variant="danger" onclick={doDelete} disabled={deleting}>
			{deleting ? 'Deleting…' : dropData ? 'Delete applet and data' : 'Delete applet'}
		</Button>
	{/snippet}
</Modal>

<style>
	.info {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.desc {
		margin: 0;
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.group-head {
		margin: 0 0 8px;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 48px;
		padding: 8px 16px;
		font-size: 14px;
		color: var(--color-foreground);
	}
	.row + .row {
		border-top: 1px solid var(--color-border);
	}
	button.row {
		width: 100%;
		border: 0;
		background: none;
		font: inherit;
		font-size: 14px;
		text-align: left;
		cursor: pointer;
		color: var(--color-foreground);
	}
	button.row:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}
	.row-label {
		flex: 1;
		min-width: 0;
	}
	.row-value {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		text-align: right;
		color: var(--color-foreground-muted);
	}
	.sub {
		font-size: 13px;
		color: var(--color-foreground-subtle);
	}
	.dots {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.history-row .row-label {
		color: var(--color-foreground-muted);
	}

	.switch {
		width: 44px;
		height: 26px;
		border-radius: 999px;
		background: var(--color-border);
		position: relative;
		flex: none;
		border: 0;
		cursor: pointer;
		transition: background 150ms ease;
		padding: 0;
	}
	.switch.on {
		background: var(--color-success);
	}
	.switch::after {
		content: '';
		position: absolute;
		top: 3px;
		left: 3px;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: var(--color-background);
		transition: transform 150ms ease;
	}
	.switch.on::after {
		transform: translateX(18px);
	}
	.switch:focus-visible {
		outline: 2px solid var(--color-foreground);
		outline-offset: 3px;
	}
	.switch:disabled {
		opacity: 0.6;
		cursor: default;
	}
	@media (prefers-reduced-motion: reduce) {
		.switch,
		.switch::after {
			transition: none;
		}
	}

	.delete-row {
		display: flex;
	}
	.error-msg {
		margin: 0;
		font-size: 13px;
		color: var(--color-error);
	}

	.del {
		display: flex;
		flex-direction: column;
		gap: 12px;
		font-size: 14px;
	}
	.del p {
		margin: 0;
	}
	.drop-opt {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		cursor: pointer;
	}
	.tbl-list {
		margin: 0;
		padding-left: 24px;
		max-height: 8rem;
		overflow-y: auto;
	}
	.del code {
		font-size: 13px;
	}
	.dim {
		color: var(--color-foreground-subtle);
	}
</style>
