<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import Card from '$lib/components/Card.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import DayDot from './DayDot.svelte';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import {
		deleteApplet,
		getAppletData,
		type Applet,
		type AppletData
	} from '$lib/api/client';
	import {
		appletDays,
		countRuns,
		DAY_LABEL,
		lastDays,
		recentSummary,
		type DayIndex
	} from '$lib/applets/days';

	/**
	 * An applet's Info: how its last 7 days went, then Technical details and
	 * Delete. Its switch and schedule are in the page's header. Run history
	 * and Technical details open in place: each is one tap from the page, and
	 * neither holds enough to need a page of its own.
	 */
	let {
		action = $bindable(),
		index,
		daysErr,
		historyOpen = $bindable(false),
		technicalOpen = $bindable(false),
		history,
		technical,
		onDeleted
	}: {
		action: Applet;
		index: DayIndex;
		daysErr: string | null;
		historyOpen?: boolean;
		technicalOpen?: boolean;
		history: Snippet;
		technical: Snippet;
		onDeleted: () => void;
	} = $props();

	let err = $state<string | null>(null);

	const week = lastDays(7);
	const states = $derived(appletDays(action, index, week));
	const counts = $derived(countRuns(index, action.id, week));
	const summary = $derived(recentSummary(states, counts));
	// Red only for what needs you: a failure, or today's missed run.
	const weekProblem = $derived((counts.get('error') ?? 0) > 0 || states.at(-1) === 'missed');
	const isSystem = $derived(action.owner === 'system');

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
	<section>
		<h3 class="group-head">Run history</h3>
		<Card list>
			<button
				type="button"
				class="row history-row"
				aria-expanded={historyOpen}
				onclick={() => (historyOpen = !historyOpen)}
			>
				{#if daysErr}
					<span class="row-label">Your server couldn't read the last 7 days</span>
				{:else}
					<span class="history-body">
						<span class="dots">
							{#each states as state, i (week[i])}
								<DayDot {state} size="md" title={`${week[i]}: ${DAY_LABEL[state]}`} />
							{/each}
						</span>
						<span class="summary" class:problem={weekProblem}>
							{summary}
						</span>
					</span>
				{/if}
				<Icon icon={historyOpen ? 'ri:arrow-up-s-line' : 'ri:arrow-down-s-line'} width="16" />
			</button>
			{#if historyOpen}
				<div class="opened">{@render history()}</div>
			{/if}
		</Card>
	</section>

	<Card list>
		<button
			type="button"
			class="row"
			aria-expanded={technicalOpen}
			onclick={() => (technicalOpen = !technicalOpen)}
		>
			<span class="row-label">Technical details</span>
			<Icon icon={technicalOpen ? 'ri:arrow-up-s-line' : 'ri:arrow-down-s-line'} width="16" />
		</button>
		{#if technicalOpen}
			<div class="opened">{@render technical()}</div>
		{/if}
	</Card>

	{#if err}
		<p class="error-msg">{err}</p>
	{/if}

	{#if !isSystem}
		<div class="delete-row">
			<Button variant="danger" size="sm" onclick={openDelete}>Delete applet</Button>
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
	.dots {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.history-row .row-label {
		color: var(--color-foreground-muted);
	}

	.opened {
		padding: 16px;
		border-top: 1px solid var(--color-border);
	}

	.delete-row {
		display: flex;
		justify-content: center;
	}
	.history-body {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 8px 0;
	}
	.summary {
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.summary.problem {
		color: var(--color-error);
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
