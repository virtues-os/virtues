<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import type { AppletLogEntry, AppletTrigger } from '$lib/api/client';
	import { failureReason, relativeTime } from '$lib/applets/palette';
	import { formatMicrosPrecise } from '$lib/utils/currency';
	import { explainRunError } from '$lib/sources/run-errors';
	import { isTauri } from '$lib/utils/platform';

	/**
	 * The last runs an applet made, newest first, as a list. Each row says
	 * when it ran, what started it, what it said or made, and what went wrong
	 * in a sentence, with the error's own text under it.
	 *
	 * There is no calendar of days and no count of failures. A grid of marked
	 * days reads as a streak, and someone in recovery sees every page.
	 *
	 * A run's row is its whole record. The server keeps what it said, what
	 * went wrong and what it cost, but not the steps it took, so there is no
	 * per-run page to open.
	 */
	let {
		log,
		collectorDenied
	}: {
		log: AppletLogEntry[];
		collectorDenied: string[];
	} = $props();

	const STARTED_BY: Record<AppletTrigger, string> = {
		cron: 'On schedule',
		manual: 'Run by you',
		tool: 'From a chat',
		api: 'From another app',
		webhook: 'From a webhook',
		message: 'Your message',
		data: 'From your data'
	};

	const STATUS_WORD: Record<AppletLogEntry['status'], string> = {
		success: 'Ran',
		error: 'Failed',
		running: 'Running',
		skipped: 'Skipped',
		cancelled: 'Cancelled',
		budget_exceeded: 'Stopped at its spending limit'
	};

	// Newest first, failures among them where they happened. The log already
	// folds identical runs into one entry. Retrying is the page header's job:
	// it sits on the problem line under the applet's name.
	const SHOWN = 8;
	const recent = $derived(log.slice(0, SHOWN));

	/** "Reached api.weather.gov (3 requests). Refused 1 to elsewhere.example.com." */
	function reachedLine(e: AppletLogEntry): string {
		const plural = (n: number) => (n === 1 ? '1 request' : `${n} requests`);
		const reached = e.requests
			.filter((h) => h.requests > h.refused)
			.map((h) => `${h.host} (${plural(h.requests - h.refused)})`);
		const refused = e.requests.filter((h) => h.refused > 0).map((h) => `${plural(h.refused)} to ${h.host}`);
		const parts = [];
		if (reached.length) parts.push(`Reached ${reached.join(', ')}.`);
		if (refused.length) parts.push(`Refused ${refused.join(', ')}.`);
		return parts.join(' ');
	}

	/** "Made Tuesday, 3 March and 3 rows in drinks." */
	function madeLine(e: AppletLogEntry): string {
		const parts = (e.made ?? []).map((m) => {
			if (m.kind === 'page') return m.title ?? 'a page';
			if (m.kind === 'task') return 'a task';
			const table = m.ref_id.split('.').pop() ?? m.ref_id;
			return `${m.count === 1 ? '1 row' : `${m.count} rows`} in ${table}`;
		});
		if (parts.length === 0) return '';
		const list = parts.length === 1 ? parts[0] : `${parts.slice(0, -1).join(', ')} and ${parts[parts.length - 1]}`;
		return `Made ${list}.`;
	}

	// Same relaxation DevicesView chose for its fix button: the deep link opens
	// THIS machine's settings, so it appears only in the native app.
	const canFix = isTauri;
</script>

{#snippet runItem(e: AppletLogEntry)}
		{@const why = e.error ? explainRunError(e.error, collectorDenied) : null}
		<li class="run">
			<div class="run-top">
				<span class="run-status">{STATUS_WORD[e.status]}</span>
				{#if e.occurrences > 1}
					<!-- The applet repeated itself: a poll that finds nothing
					     still records a run. One row stands for all of them,
					     and the line under it says from when to when. -->
					<span class="run-meta">More than once</span>
				{:else if e.trigger}
					<span class="run-meta">{STARTED_BY[e.trigger]}</span>
				{/if}
				{#if e.cost_micros}
					<span class="run-meta" title="What these runs spent with the model">
						{formatMicrosPrecise(e.cost_micros)}
					</span>
				{/if}
				<span class="run-time">{relativeTime(e.last_at)}</span>
			</div>
			{#if e.message}
				<p class="run-said">{e.message}</p>
			{/if}
			{#if e.summary}
				<p class="run-text">{e.summary}</p>
			{/if}
			{#if e.made?.length}
				<p class="run-span">{madeLine(e)}</p>
			{/if}
			{#if e.requests?.length}
				<!-- Where its code reached through the egress proxy, so where
				     its data went is on the run. -->
				<p class="run-span">{reachedLine(e)}</p>
			{/if}
			{#if e.error}
				<!-- A permission failure is a checkbox in System Settings,
				     not a stack trace: name the condition and the fix, and
				     keep the OS's sentence beneath it as evidence. -->
				{#if why}
					<p class="run-text"><Icon icon="mynaui:lock" width="12" /> {why.title}. {why.remedy}</p>
					{#if canFix && why.open}
						<Button variant="secondary" onclick={() => why.open?.()}>
							Open {why.label} on this computer
						</Button>
					{/if}
				{:else}
					<p class="run-text">{failureReason(e.error)}.</p>
				{/if}
				<pre class="run-error">{e.error}</pre>
			{/if}
			{#if e.occurrences > 1 && e.first_at}
				<p class="run-span">{relativeTime(e.first_at)} to {relativeTime(e.last_at)}</p>
			{/if}
				</li>
{/snippet}

<div class="history">
	{#if recent.length === 0}
		<p class="note">No runs yet.</p>
	{:else}
		<ul class="runs" role="list">
			{#each recent as e (e.run_id ?? e.last_at)}{@render runItem(e)}{/each}
		</ul>
	{/if}
</div>

<style>
	.history {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.note {
		margin: 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.runs {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.run {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 12px 0;
		border-bottom: 1px solid var(--color-border);
	}
	.run:last-child {
		border-bottom: 0;
		padding-bottom: 0;
	}
	.run-top {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
	}
	.run-status {
		color: var(--color-foreground);
		font-weight: 500;
	}
	.run-meta,
	.run-time,
	.run-span {
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}
	.run-time {
		margin-left: auto;
	}
	.run-span {
		margin: 0;
		font-size: 12px;
	}
	/* What the person said, set apart from what the applet answered. */
	.run-said {
		margin: 0;
		padding-left: 8px;
		border-left: 2px solid var(--color-border);
		font-size: 13px;
	}
	.run-text {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.run-error {
		margin: 0;
		font-family: var(--font-sans);
		font-size: 12px;
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--color-foreground-muted);
	}
</style>
