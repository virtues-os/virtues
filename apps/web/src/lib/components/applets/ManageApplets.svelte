<script lang="ts">
	/**
	 * Settings > Applets: every applet on the server, built-in ones included,
	 * with when it runs and how its runs went. For checking and fixing; the
	 * Applets page in Home is where you use them.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import MenuItem from '$lib/components/MenuItem.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { FilterDef } from '$lib/components/datagrid/types';
	import {
		adminReconcile,
		getRunsByDay,
		runApplet,
		type Applet,
		type RunDay
	} from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { appletDestination, appletGlyph, describeSchedule, relativeTime } from '$lib/applets/palette';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import { appletDays, DAY_LABEL, indexRunDays, lastDays, reliabilityScore, type DayState } from '$lib/applets/days';
	import DayDot from './DayDot.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { pinMenuItem } from '$lib/pins/pinAction';
	import type { Tab } from '$lib/tabs/types';

	let { tab }: { tab: Tab } = $props();

	// Reached from the Applets list: offer the way back to it.
	const fromList = $derived(tab.history?.[tab.historyIndex - 1] === '/applets');
	// Back in this tab's own pane: opening a row puts Info in the other pane
	// and makes that one active, so a bare goBack() would step the wrong tab.
	function back() {
		windowShellStore.goBack(windowShellStore.panes.find((p) => p.tabs.some((t) => t.id === tab.id))?.id);
	}

	const applets = $derived(appletsStore.list);
	let runDays = $state<RunDay[]>([]);
	// The day counts come from their own endpoint. If it fails, the list still
	// loads and the column says it couldn't read them, rather than showing
	// seven empty days that would read as "nothing ran".
	let runDaysErr = $state<string | null>(null);
	let loading = $state(true);
	let err = $state<string | null>(null);
	let moreMenuOpen = $state(false);
	let reconciling = $state(false);
	let reconcileMsg = $state<string | null>(null);
	let retrying = $state<string | null>(null);
	// Finished applets (lifecycle complete) are out of the way, not gone: a
	// one-shot reminder that fired keeps its output and its runs.
	let showFinished = $state(false);

	const finished = $derived(applets.filter((a) => a.archived_at));
	const living = $derived(applets.filter((a) => !a.archived_at));

	const visible = $derived(showFinished ? [...living, ...finished] : living);

	// One dot per day for the last week, oldest on the left. Every applet
	// shares this axis, so the column reads down the list whatever each one's
	// schedule is.
	const WEEK = 7;
	const week = $derived(lastDays(WEEK));
	const dayIndex = $derived(indexRunDays(runDays));
	const daysOf = (a: Applet): DayState[] => appletDays(a, dayIndex, week);

	/** The last run's outcome, as the row shows it. */
	type Outcome = 'failed' | 'stopped' | 'ran' | 'running' | 'off' | 'finished' | 'never';
	function outcomeOf(a: Applet): Outcome {
		if (a.archived_at) return 'finished';
		if (!a.enabled) return 'off';
		const s = a.last_run?.status;
		if (!s) return 'never';
		if (s === 'error') return 'failed';
		if (s === 'budget_exceeded') return 'stopped';
		if (s === 'running') return 'running';
		return 'ran';
	}
	const OUTCOME_ICON: Record<Outcome, string> = {
		failed: 'ri:close-circle-line',
		stopped: 'ri:error-warning-line',
		ran: 'ri:checkbox-circle-line',
		running: 'ri:loader-4-line',
		off: 'ri:subtract-line',
		finished: 'ri:flag-line',
		never: 'ri:subtract-line'
	};
	// Problems sort first, then everything that ran, then the quiet rest.
	const OUTCOME_RANK: Record<Outcome, number> = {
		failed: 0,
		stopped: 1,
		running: 2,
		ran: 3,
		never: 4,
		off: 5,
		finished: 6
	};

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
		runDaysErr = null;
		const [, days] = await Promise.allSettled([appletsStore.load(), getRunsByDay(30)]);
		// The table says what failed in words; the raw reason ("Bad Gateway")
		// is no help to the person reading it.
		err = appletsStore.error ? "Your server couldn't send your applets. Reload the page to try again." : null;
		if (days.status === 'fulfilled') runDays = days.value;
		else runDaysErr = days.reason instanceof Error ? days.reason.message : String(days.reason);
		loading = false;
	}

	$effect(() => {
		void load();
	});

	function openHome(a: Applet) {
		windowShellStore.openAside({
			type: 'applet',
			label: a.name,
			route: `/applet/${a.id}`,
			icon: 'atlas:applets'
		});
	}

	function openHistory(a: Applet) {
		windowShellStore.openAside({
			type: 'applet',
			label: a.name,
			route: `/applet/${a.id}?panel=history`,
			icon: 'atlas:applets'
		});
	}

	function chatOf(a: Applet): string | null {
		const id = a.config?.chat_id;
		return typeof id === 'string' && id ? id : null;
	}

	function openConversation(a: Applet) {
		const chat = chatOf(a);
		if (!chat) return;
		windowShellStore.openAside({ type: 'chat', label: a.name, route: `/chat/${chat}`, icon: 'atlas:applets' });
	}

	// A row opens the applet's page beside the table, which carries its Info;
	// Run history and Technical details are one step on.
	function openRow(a: Applet) {
		windowShellStore.openAside({
			type: 'applet',
			label: a.name,
			route: `/applet/${a.id}`,
			icon: 'atlas:applets'
		});
	}

	async function retry(a: Applet) {
		retrying = a.id;
		try {
			await runApplet(a.id);
			await load();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			retrying = null;
		}
	}

	function rowContextMenu(a: Applet, e: MouseEvent) {
		e.preventDefault();
		const items: ContextMenuItem[] = [];
		if (chatOf(a)) {
			items.push({ id: 'chat', label: 'Open conversation', icon: 'ri:chat-3-line', action: () => openConversation(a) });
		}
		items.push({ id: 'home', label: 'Open applet', icon: 'atlas:applets', action: () => openHome(a) });
		items.push({ id: 'history', label: 'Run history', icon: 'ri:history-line', action: () => openHistory(a) });
		if (a.enabled && !a.archived_at) {
			items.push({ id: 'run', label: 'Run now', icon: 'ri:play-line', action: () => void retry(a) });
		}
		items.push(pinMenuItem({ url: `/applet/${a.id}`, label: a.name, icon: 'atlas:applets' }));
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	function runsLabel(a: Applet): string {
		if (a.archived_at) return 'Finished';
		if (!a.enabled) return 'Off';
		return describeSchedule(a.schedule);
	}

	// What made this applet, in the user's terms. "Source" is the one that was
	// unsayable before: those rows are owner='system' and read as built-in, but
	// they exist because the user connected something.
	const ORIGIN_LABEL: Record<string, string> = {
		source: 'Source',
		ai: 'AI-authored',
		user: 'You',
		system: 'Built-in'
	};

	// The values here drive sorting and search; `tableRow` below draws the
	// cells. Origin and Lifecycle stay as hidden columns so their filters and
	// search keep working.
	const columns: Column<Applet>[] = [
		{ key: 'name', label: 'Name', width: '30%', minWidth: '200px' },
		{
			key: 'config',
			label: 'Goes to',
			width: '22%',
			minWidth: '130px',
			getValue: (a) => appletDestination(a)
		},
		{
			key: 'schedule',
			label: 'Runs',
			width: '16%',
			minWidth: '130px',
			getValue: (a) => runsLabel(a)
		},
		{
			key: 'pulse',
			label: 'Last 7 days',
			width: '128px',
			// Sorting by this column puts the least reliable applets first.
			getValue: (a) => -reliabilityScore(daysOf(a))
		},
		{
			key: 'last_run',
			label: 'Last run',
			minWidth: '150px',
			getValue: (a) => OUTCOME_RANK[outcomeOf(a)]
		},
		{
			key: 'origin',
			label: 'Origin',
			hidden: true,
			getValue: (a) => ORIGIN_LABEL[a.origin] ?? a.origin
		},
		{
			key: 'until',
			label: 'Lifecycle',
			hidden: true,
			getValue: (a) => (!a.until ? 'forever' : a.until.toLowerCase() === 'once' ? 'once' : 'until')
		}
	];

	const filters: FilterDef<Applet>[] = [
		{
			id: 'origin',
			kind: 'multi',
			label: 'Origin',
			options: [
				{ value: 'source', label: ORIGIN_LABEL.source },
				{ value: 'ai', label: ORIGIN_LABEL.ai },
				{ value: 'user', label: ORIGIN_LABEL.user },
				{ value: 'system', label: ORIGIN_LABEL.system }
			],
			predicate: (a, v) => Array.isArray(v) && v.includes(a.origin)
		},
		{
			id: 'enabled',
			kind: 'enum',
			label: 'Status',
			options: [
				{ value: 'true', label: 'On', badgeColor: 'badge-success' },
				{ value: 'false', label: 'Off', badgeColor: 'badge-muted' },
				{ value: 'finished', label: 'Finished', badgeColor: 'badge-info' }
			],
			predicate: (a, v) =>
				v === 'finished' ? Boolean(a.archived_at) : !a.archived_at && String(a.enabled) === v
		},
		{
			id: 'last_run_status',
			kind: 'enum',
			label: 'Last run',
			options: [
				{ value: 'success', label: 'Ran', badgeColor: 'badge-success' },
				{ value: 'error', label: 'Failed', badgeColor: 'badge-error' },
				{ value: 'running', label: 'Running', badgeColor: 'badge-warning' },
				{ value: 'budget_exceeded', label: 'Stopped at limit', badgeColor: 'badge-warning' }
			],
			predicate: (a, v) => (a.last_run?.status ?? null) === v
		}
	];

	// Problems first, as the list opens. Any column header re-sorts it.
	const ordered = $derived(
		[...visible].sort(
			(a, b) => OUTCOME_RANK[outcomeOf(a)] - OUTCOME_RANK[outcomeOf(b)] || a.name.localeCompare(b.name)
		)
	);
</script>

<section class="applets-panel">
	{#if fromList}
		<button type="button" class="backlink" onclick={back}>
			<Icon icon="ri:arrow-left-line" width="14" />
			Back to Applets
		</button>
	{/if}
	<header class="section-header">
		<div>
			<h2>{fromList ? 'Manage applets' : 'Applets'}</h2>
			<p class="subtitle">
				Every applet on this server, built-in ones included, with when it runs and how its runs went.
			</p>
		</div>
		<div class="header-applets">
			{#if reconcileMsg}
				<span class="reconcile-msg">{reconcileMsg}</span>
			{/if}
			<!-- Reconcile is an operator verb, so it lives behind the overflow:
			     reachable, not offered. -->
			<Popover bind:open={moreMenuOpen} placement="bottom-end" offset={4}>
				{#snippet trigger({ toggle })}
					<IconButton
						icon="ri:more-2-fill"
						label="More"
						size="md"
						variant="secondary"
						expanded={moreMenuOpen}
						haspopup="menu"
						onclick={toggle}
					/>
				{/snippet}
				{#snippet children()}
					<div class="new-menu" role="menu">
						<MenuItem
							icon="ri:refresh-line"
							label="Re-read from disk"
							description="Pick up applet folders that changed outside the app"
							loading={reconciling}
							onclick={() => {
								moreMenuOpen = false;
								void reconcile();
							}}
						/>
					</div>
				{/snippet}
			</Popover>
		</div>
	</header>

	{#if runDaysErr}
		<!-- Said once for the table, not in every row. -->
		<p class="days-note">Your server couldn't send the last 7 days, so that column is empty.</p>
	{/if}
	<div class="table-card">
	<UniversalDataGrid
		items={ordered}
		{columns}
		{filters}
		entityType="applets"
		defaultViewMode="table"
		viewModes={['table']}
		mobileViewMode="table"
		{loading}
		error={err}
		emptyIcon="atlas:applets"
		emptyMessage="Nothing runs for you yet. Ask in chat - “write my examen each morning,” “remind me on the 25th,” “a dashboard of my heart rate”, and it becomes an applet."
		searchPlaceholder="Search applets…"
		pageSize={50}
		onItemClick={openRow}
		onItemContextMenu={rowContextMenu}
	>
		{#snippet tableRow(a)}
			{@const outcome = outcomeOf(a)}
			<td>
				<span class="name-cell">
					<span class="tile" aria-hidden="true"><AtlasIcon name={appletGlyph(a)} size={14} bare /></span>
					<span class="name" class:muted={!a.enabled || a.archived_at}>{a.name}</span>
					<!-- Built-in applets keep the server running; a source's applets
					     came with something you connected. Said in words, not only
					     by the icon. -->
					{#if a.origin === 'system' || a.origin === 'source'}
						<span class="origin-tag">{ORIGIN_LABEL[a.origin]}</span>
					{/if}
				</span>
			</td>
			<td>
				<!-- Where its work goes, as a way there: the applet's page. An
				     applet that doesn't say gets nothing rather than a dash. -->
				{#if appletDestination(a) !== '-'}
				<button
					type="button"
					class="goes"
					class:muted={!a.enabled || a.archived_at}
					title={`Open ${a.name}`}
					onclick={(e) => {
						e.stopPropagation();
						openHome(a);
					}}
				>
					<AtlasIcon name={appletGlyph(a)} size={14} bare />
					<span class="goes-text">{appletDestination(a)}</span>
				</button>
				{/if}
			</td>
			<td>
				<span class="runs" class:muted={!a.enabled || a.archived_at}>{runsLabel(a)}</span>
			</td>
			<td class="week-cell">
				{#if runDaysErr || !a.enabled || a.archived_at}
					<span class="muted">-</span>
				{:else}
					<button
						type="button"
						class="week"
						title="Show run history"
						aria-label={`Last 7 days for ${a.name}. Show run history`}
						onclick={(e) => {
							e.stopPropagation();
							openHistory(a);
						}}
					>
						{#each daysOf(a) as state, i (week[i])}
							<DayDot {state} title={`${week[i]}: ${DAY_LABEL[state]}`} />
						{/each}
						<span class="week-go" aria-hidden="true">›</span>
					</button>
				{/if}
			</td>
			<td class="last-cell">
				<button
					type="button"
					class="last {outcome}"
					title="Show this run"
					onclick={(e) => {
						e.stopPropagation();
						openHistory(a);
					}}
				>
					<Icon icon={OUTCOME_ICON[outcome]} width="15" />
					<span>
						{#if outcome === 'off'}Off{:else if outcome === 'finished'}Finished{:else if outcome === 'never'}Never{:else}{relativeTime(a.last_run?.started_at)}{/if}
					</span>
				</button>
				{#if outcome === 'failed'}
					<Button
						variant="secondary"
						size="sm"
						disabled={retrying === a.id}
						onclick={(e: MouseEvent) => {
							e.stopPropagation();
							void retry(a);
						}}
					>
						{retrying === a.id ? 'Retrying…' : 'Retry'}
					</Button>
				{/if}
			</td>
		{/snippet}
	</UniversalDataGrid>
	</div>

	{#if finished.length > 0}
		<div class="finished-row">
			<Button variant="secondary" size="sm" onclick={() => (showFinished = !showFinished)}>
				{showFinished ? 'Hide finished' : `Show finished (${finished.length})`}
			</Button>
		</div>
	{/if}
</section>


<style>
	.applets-panel {
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
	/* Matches PageHeading's level-1 title and its description, so a hand-rolled
	   header still reads as a page title (agents/build/design-grammar.md §4). */
	.section-header h2 {
		margin: 0;
		font-family: var(--font-serif, ui-serif, Georgia, serif);
		font-size: 36px;
		line-height: 1.15;
		font-weight: 400;
	}
	.subtitle {
		margin: 0.5rem 0 0;
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}
	.header-applets {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}
	.new-menu {
		display: flex;
		flex-direction: column;
		min-width: 240px;
		padding: 0.25rem;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
	}
	.reconcile-msg {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}
	.name {
		font-family: var(--font-serif, ui-serif, Georgia, serif);
		font-size: 1rem;
		color: var(--color-foreground);
	}
	.backlink {
		align-self: flex-start;
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: 13px;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.backlink:hover {
		color: var(--color-foreground);
	}
	/* The table sits on a card, as the prototype draws it: a list of things
	   you can act on, not a run of hairlines across the page. */
	.days-note {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.table-card {
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
		overflow: hidden;
		padding: 0 16px 8px;
	}
	.name-cell {
		display: inline-flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
	}
	.origin-tag {
		flex: none;
		padding: 0 8px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		font-size: 12px;
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.tile {
		width: 28px;
		height: 28px;
		flex: none;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 6px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground-muted);
	}
	/* Rows tall enough for the Retry button they can carry. */
	td {
		height: 48px;
	}
	.goes {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		max-width: 100%;
		padding: 4px 12px;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		background: none;
		font: inherit;
		font-size: 13px;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.goes:hover {
		color: var(--color-foreground);
		border-color: var(--color-border-strong);
	}
	.goes-text {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.runs {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.muted {
		color: var(--color-foreground-subtle);
	}

	/* The whole cell is one button, so the dots are easy to hit. Its outline
	   shows on row hover and lifts on its own hover. */
	.week {
		display: flex;
		align-items: center;
		gap: 4px;
		width: 100%;
		min-height: 32px;
		padding: 0 8px;
		margin: 0 -8px;
		border: 1px solid transparent;
		border-radius: 6px;
		background: none;
		cursor: pointer;
	}
	:global(tr:hover) .week {
		border-color: var(--color-border);
	}
	.week:hover {
		border-color: var(--color-border-strong);
		background: var(--color-surface);
	}
	.week:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: -2px;
	}
	.week-go {
		margin-left: auto;
		color: var(--color-foreground-subtle);
		opacity: 0;
	}
	:global(tr:hover) .week-go,
	.week:focus-visible .week-go {
		opacity: 1;
	}
	.last-cell {
		white-space: nowrap;
	}
	.last-cell :global(button + button) {
		margin-left: 0.5rem;
	}
	.last {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 2px 6px;
		margin: 0 -6px;
		border: none;
		border-radius: 6px;
		background: none;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.last:hover {
		background: var(--color-surface-elevated);
	}
	.last.failed {
		color: var(--color-error);
	}
	.last.stopped {
		color: var(--color-warning);
	}
	.last.ran {
		color: var(--color-foreground-muted);
	}
	.last.ran :global(svg) {
		color: var(--color-success);
	}
	.finished-row {
		display: flex;
	}
</style>
