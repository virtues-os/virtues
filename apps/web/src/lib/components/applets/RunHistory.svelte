<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import DayDot from './DayDot.svelte';
	import type { Applet, AppletLogEntry, AppletTrigger } from '$lib/api/client';
	import { relativeTime } from '$lib/applets/palette';
	import { formatMicrosPrecise } from '$lib/utils/currency';
	import {
		appletDays,
		busiestDay,
		DAY_LABEL,
		dayKey,
		lastDays,
		stateOf,
		type DayIndex,
		type DayState
	} from '$lib/applets/days';
	import { explainRunError } from '$lib/sources/run-errors';
	import { isTauri } from '$lib/utils/platform';

	/**
	 * Every run an applet made recently. Days first, so a pattern shows (it
	 * fails every Monday; it stopped on the 3rd), then the runs themselves.
	 *
	 * An applet that runs about once a day gets a calendar, one dot a day. One
	 * that runs many times a day gets a row per day, one dot a run, because a
	 * single dot would hide 23 good runs behind one bad one.
	 *
	 * A run's row is its whole record. The server keeps what it said, what
	 * went wrong and what it cost, but not the steps it took, so there is no
	 * per-run page to open.
	 */
	let {
		action,
		log,
		index,
		daysErr,
		collectorDenied,
		busy,
		onRetry,
		onAsk
	}: {
		action: Applet;
		log: AppletLogEntry[];
		index: DayIndex;
		daysErr: string | null;
		collectorDenied: string[];
		busy: boolean;
		/** Present only when the applet can be run by hand. */
		onRetry?: () => void;
		/** Present only when the applet has a conversation. */
		onAsk?: () => void;
	} = $props();

	// Weeks start on Monday. Four whole weeks plus this one so far, which
	// stays inside the 35 days the home loads.
	const today = new Date();
	const weekday = (today.getDay() + 6) % 7;
	const shown = lastDays(28 + weekday + 1, today);
	const trailing = 6 - weekday;
	// The rest of this week, so the calendar ends on a Sunday like a calendar.
	const future = Array.from({ length: trailing }, (_, i) => {
		const d = new Date(today);
		d.setDate(d.getDate() + i + 1);
		return d.getDate();
	});

	const states = $derived(appletDays(action, index, shown));
	const frequent = $derived(busiestDay(index, action.id) > 1);
	// Newest first, for the rows.
	const recentDays = $derived(shown.slice(-14).reverse());

	function dateOf(key: string): Date {
		const [y, m, d] = key.split('-').map(Number);
		return new Date(y, m - 1, d);
	}

	function dayName(key: string): string {
		const t = dayKey(today);
		if (key === t) return 'Today';
		const y = new Date(today);
		y.setDate(y.getDate() - 1);
		if (key === dayKey(y)) return 'Yesterday';
		return dateOf(key).toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
	}

	/** One dot per run, worst outcomes first, so a day's problems lead the row. */
	function runDots(key: string): DayState[] {
		const order: [string, DayState][] = [
			['error', 'failed'],
			['budget_exceeded', 'stopped'],
			['success', 'ran'],
			['running', 'ran'],
			['skipped', 'quiet'],
			['cancelled', 'quiet']
		];
		const statuses = index.get(action.id)?.get(key);
		const out: DayState[] = [];
		for (const [status, state] of order) {
			for (let i = 0; i < (statuses?.get(status) ?? 0); i++) out.push(state);
		}
		return out;
	}

	function daySentence(key: string): string {
		const statuses = index.get(action.id)?.get(key);
		let total = 0;
		for (const n of statuses?.values() ?? []) total += n;
		if (total === 0) return 'No runs';
		const failed = statuses?.get('error') ?? 0;
		const runs = `${total} ${total === 1 ? 'run' : 'runs'}`;
		return failed ? `${runs}, ${failed} failed` : runs;
	}

	const MAX_DOTS = 48;

	const entryState = (e: AppletLogEntry): DayState =>
		stateOf(new Map([[e.status, 1]]));

	const STARTED_BY: Record<AppletTrigger, string> = {
		cron: 'On schedule',
		manual: 'Run by you',
		tool: 'From a chat',
		api: 'From another app',
		webhook: 'From a webhook',
		message: 'Your message'
	};

	const STATUS_WORD: Record<AppletLogEntry['status'], string> = {
		success: 'Ran',
		error: 'Failed',
		running: 'Running',
		skipped: 'Skipped',
		cancelled: 'Cancelled',
		budget_exceeded: 'Stopped at its spending limit'
	};

	// What needs looking at, then the newest of the rest. The log already
	// folds identical runs into one entry with a count.
	const isProblem = (e: AppletLogEntry) => e.status === 'error' || e.status === 'budget_exceeded';
	const problems = $derived(log.filter(isProblem));
	const latest = $derived(log.filter((e) => !isProblem(e)).slice(0, 5));

	// Only the newest run can be retried: retrying an older failure would
	// run the applet now, which is what Retry on the newest one does anyway.
	const newestFailed = $derived(log[0]?.status === 'error' ? log[0] : null);

	// Same relaxation DevicesView chose for its fix button: the deep link opens
	// THIS machine's settings, so it appears only in the native app.
	const canFix = isTauri;
</script>

{#snippet runItem(e: AppletLogEntry)}
		{@const why = e.error ? explainRunError(e.error, collectorDenied) : null}
		<li class="run">
			<div class="run-top">
				<DayDot state={entryState(e)} />
				<span class="run-status">{STATUS_WORD[e.status]}</span>
				{#if e.occurrences > 1}
					<!-- The applet repeated itself. A poll that finds nothing
					     still records a run, so saying it once with a count is
					     shorter and more honest than 600 identical lines. -->
					<span class="run-meta">{e.occurrences} times</span>
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
			{#if e.error}
				<!-- A permission failure is a checkbox in System Settings,
				     not a stack trace: name the condition and the fix, and
				     keep the OS's sentence beneath it as evidence. -->
				{#if why}
					<p class="run-text"><Icon icon="ri:lock-line" width="12" /> {why.title}. {why.remedy}</p>
					{#if canFix && why.open}
						<Button variant="secondary" size="sm" onclick={() => why.open?.()}>
							Open {why.label} on this computer
						</Button>
					{/if}
				{/if}
				<pre class="run-error">{e.error}</pre>
			{/if}
			{#if e.occurrences > 1 && e.first_at}
				<p class="run-span">{relativeTime(e.first_at)} to {relativeTime(e.last_at)}</p>
			{/if}
			{#if e === newestFailed && (onRetry || onAsk)}
				<div class="run-actions">
					{#if onRetry}
						<Button variant="secondary" size="sm" onclick={onRetry} disabled={busy}>Retry</Button>
					{/if}
					{#if onAsk}
						<Button variant="secondary" size="sm" onclick={onAsk}>Ask why in its conversation</Button>
					{/if}
				</div>
			{/if}
		</li>
{/snippet}

<div class="history">
	{#if daysErr}
		<p class="note">Your server couldn't read the run counts, so the days are missing. The runs below are complete.</p>
	{:else if !frequent}
		<div class="cal">
			{#each ['M', 'T', 'W', 'T', 'F', 'S', 'S'] as d, i (i)}
				<span class="cal-head" aria-hidden="true">{d}</span>
			{/each}
			{#each shown as key, i (key)}
				<span class="cal-cell {states[i]}" title={`${dayName(key)}: ${DAY_LABEL[states[i]]}`}>
					{dateOf(key).getDate()}
				</span>
			{/each}
			{#each future as d (d)}
				<span class="cal-cell future" aria-hidden="true">{d}</span>
			{/each}
		</div>
		<p class="legend">
			<span><span class="key ran"></span>Ran</span>
			<span><span class="key failed"></span>Failed</span>
			<span><span class="key stopped"></span>Stopped at its limit</span>
			<span><span class="key missed"></span>Didn't run when due</span>
			<span><span class="key none"></span>No run</span>
		</p>
	{:else}
		<div class="strips">
			{#each recentDays as key (key)}
				{@const dots = runDots(key)}
				<div class="strip">
					<span class="strip-date">{dayName(key)}</span>
					<span class="strip-dots">
						{#each dots.slice(0, MAX_DOTS) as state, i (i)}
							<DayDot {state} />
						{/each}
						{#if dots.length > MAX_DOTS}
							<span class="more">+{dots.length - MAX_DOTS}</span>
						{/if}
					</span>
					<span class="strip-sum">{daySentence(key)}</span>
				</div>
			{/each}
		</div>
	{/if}

	<h3 class="runs-head">Problems</h3>
	{#if problems.length === 0}
		<p class="note">{log.length === 0 ? 'No runs yet.' : 'None in its recent runs.'}</p>
	{:else}
		<ul class="runs" role="list">
			{#each problems as e (e.run_id ?? e.last_at)}{@render runItem(e)}{/each}
		</ul>
	{/if}
	{#if latest.length > 0}
		<h3 class="runs-head">Latest runs</h3>
		<ul class="runs" role="list">
			{#each latest as e (e.run_id ?? e.last_at)}{@render runItem(e)}{/each}
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

	.cal {
		display: grid;
		grid-template-columns: repeat(7, 40px);
		gap: 8px;
	}
	.cal-head {
		font-size: 12px;
		text-align: center;
		color: var(--color-foreground-subtle);
	}
	/* A date tile per day, filled with how the day went: the prototype's
	   calendar, on the theme's state tokens. */
	.cal-cell {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 32px;
		border-radius: 6px;
		box-sizing: border-box;
		font-size: 13px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
		border: 1px solid var(--color-border);
	}
	.cal-cell.ran,
	.cal-cell.failed,
	.cal-cell.stopped {
		border-color: transparent;
		color: var(--color-background);
		font-weight: 500;
	}
	.cal-cell.ran {
		background: var(--color-success);
	}
	.cal-cell.failed {
		background: var(--color-error);
	}
	.cal-cell.stopped {
		background: var(--color-warning);
	}
	.cal-cell.quiet {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		border-color: transparent;
	}
	.cal-cell.missed {
		border: 2px solid var(--color-error);
		color: var(--color-error);
	}
	.cal-cell.future {
		border-color: transparent;
		color: var(--color-foreground-subtle);
	}
	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 16px;
		margin: 0;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.legend > span {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.key {
		width: 14px;
		height: 14px;
		border-radius: 6px;
		box-sizing: border-box;
	}
	.key.ran {
		background: var(--color-success);
	}
	.key.failed {
		background: var(--color-error);
	}
	.key.stopped {
		background: var(--color-warning);
	}
	.key.missed {
		border: 2px solid var(--color-error);
	}
	.key.none {
		border: 1px solid var(--color-border-strong);
	}

	.strips {
		display: flex;
		flex-direction: column;
	}
	.strip {
		display: grid;
		grid-template-columns: 120px minmax(0, 1fr) auto;
		align-items: center;
		gap: 12px;
		min-height: 32px;
		border-bottom: 1px solid var(--color-border);
		font-size: 13px;
	}
	.strip-date {
		color: var(--color-foreground);
	}
	.strip-dots {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px;
	}
	.strip-sum,
	.more {
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.runs-head {
		margin: 8px 0 0;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
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
	.run-actions {
		display: flex;
		gap: 8px;
		margin-top: 4px;
	}
</style>
