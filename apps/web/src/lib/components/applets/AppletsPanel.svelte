<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { FilterDef } from '$lib/components/datagrid/types';
	import { listApplets, adminReconcile, type Applet } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { describeSchedule, relativeTime } from '$lib/applets/palette';
	import AppletCard from './AppletCard.svelte';
	import GitImportModal from './GitImportModal.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';

	let applets = $state<Applet[]>([]);
	let loading = $state(true);
	let err = $state<string | null>(null);
	let newMenuOpen = $state(false);
	let moreMenuOpen = $state(false);
	let gitImportOpen = $state(false);
	let reconciling = $state(false);
	let reconcileMsg = $state<string | null>(null);
	// Built-in (system) applets are plumbing — inspectable on demand, hidden
	// by default so they don't crowd out yours. A filter, not a wall.
	let showSystem = $state(false);
	// Finished applets (lifecycle complete) are out of the way, not gone. They
	// used to be filtered out with no way back: no archived filter, no history
	// route since the phase-1 collapse, and the detail page's "Finished" branch
	// reachable only by typing the URL. So a one-shot reminder fired, archived
	// itself, and took its own output out of the interface — while the MODEL
	// could still list it (`list_applets` has include_archived) and the person
	// could not.
	let showFinished = $state(false);

	const finished = $derived(applets.filter((a) => a.archived_at));
	const living = $derived(applets.filter((a) => !a.archived_at));

	// Hide on `origin`, not `owner`. Every source fan-out row is owner='system'
	// — that field is reconcile's write-authority, not provenance — so hiding
	// by owner filed the Gmail sync you connected on purpose with the embedding
	// indexer you have never thought about. `origin === 'system'` is the actual
	// plumbing; a source's applets are yours and stay visible.
	const systemCount = $derived(living.filter((a) => a.origin === 'system').length);
	const pool = $derived(showFinished ? [...living, ...finished] : living);
	const visible = $derived(showSystem ? pool : pool.filter((a) => a.origin !== 'system'));

	// Needs-attention strip. Two signals now; credential-expired joins when
	// credential surfacing lands.
	//
	// `budget_exceeded` is deliberately NOT here: a run stopped at a ceiling
	// its owner set is working as configured, and filing it beside genuine
	// breakage teaches people to ignore the strip.
	//
	// An hour is the grace, matching the scheduler's own ceiling — long enough
	// that a busy box isn't accused, short enough that a daily applet which
	// silently stopped firing is caught the same morning.
	const OVERDUE_GRACE_MS = 60 * 60 * 1000;

	type Attention = { applet: Applet; why: string };

	const needsAttention = $derived.by((): Attention[] => {
		const out: Attention[] = [];
		for (const a of living) {
			if (!a.enabled) continue;
			if (a.last_run?.status === 'error') {
				out.push({ applet: a, why: 'last run failed' });
				continue;
			}
			// Silent non-execution: the slot passed and nothing ran. This is
			// the failure the strip could not see before — an unschedulable
			// cron, a job that never registered, a box that was off. It looks
			// identical to health in every other view.
			if (a.next_due_at) {
				const late = Date.now() - new Date(a.next_due_at).getTime();
				if (late > OVERDUE_GRACE_MS) {
					out.push({ applet: a, why: `expected ${relativeTime(a.next_due_at)}` });
				}
			}
		}
		return out;
	});

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
			// One request. This used to fan out two more per applet — about
			// fifty on a page — for the pulse and the last output, both of
			// which the list query now carries.
			applets = await listApplets();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
	});

	function openView(a: Applet) {
		windowShellStore.openTabFromRoute(`/applet/${a.id}/view`);
	}

	function openDetail(a: Applet) {
		windowShellStore.openAside({
			type: 'applet',
			label: a.name,
			route: `/applet/${a.id}`,
			icon: 'ri:flashlight-line'
		});
	}

	// Default open: wherever you can actually use the thing.
	//
	// An applet you can talk to opens its detail page, because that is where
	// the composer lives — a tracker has a face AND takes messages, and
	// sending it to the full-page face landed you somewhere you could look at
	// it but not say anything to it. A face you can only look at still opens
	// full-page. Everything else opens its settings.
	function openCard(a: Applet) {
		if (a.triggers?.includes('message')) openDetail(a);
		else if (a.has_face) openView(a);
		else openDetail(a);
	}

	// Right-click: pick view (if it has one) or settings explicitly.
	function rowContextMenu(a: Applet, e: MouseEvent) {
		e.preventDefault();
		const items = [];
		if (a.has_face) {
			items.push({
				id: 'view',
				label: 'Open view',
				icon: 'ri:layout-2-line',
				applet: () => openView(a)
			});
		}
		items.push({
			id: 'detail',
			label: 'Settings & runs',
			icon: 'ri:settings-3-line',
			applet: () => openDetail(a)
		});
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	function lastRunStatus(applet: Applet): string {
		const lr = applet.last_run;
		if (!lr) return '—';
		return lr.status ?? '—';
	}

	function lifecycleLabel(a: Applet): string {
		if (!a.until) return 'forever';
		return a.until.toLowerCase() === 'once' ? 'once' : 'until';
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

	const columns: Column<Applet>[] = [
		{ key: 'name', label: 'Name', width: '22%', minWidth: '140px' },
		{
			// The row's whole informational payload. Without it the table is six
			// columns of machine vocabulary and a name the reader could already
			// guess — Origin and Lifecycle badges answering questions nobody
			// asked, while "what is this thing for" went unanswered.
			key: 'description',
			label: 'What it does',
			width: '32%',
			minWidth: '200px',
			getValue: (a) => a.description ?? '—'
		},
		// Origin and Lifecycle are filters, not columns. As columns they were
		// two badges of provenance and bookkeeping answering questions nobody
		// walks up to this page with, in the space where "what is this for"
		// belonged. Both remain in the filter bar and in search.
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
			getValue: (a) => lifecycleLabel(a)
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
			key: 'next_due_at',
			label: 'Next run',
			// The scheduler's own answer, not a re-derivation from the cron
			// string — so a row that stopped firing reads as overdue here
			// rather than confidently predicting a run that will never come.
			getValue: (a) => (a.next_due_at ? relativeTime(a.next_due_at) : '—')
		},
		// Two facts, two columns. One column headed "Status", keyed on `enabled`
		// and rendering the last RUN's status, answered neither question: an
		// applet you had switched off still showed "Success" from whenever it
		// last ran, and whether it was on at all could only be discovered
		// through a filter.
		{
			key: 'enabled',
			label: 'On',
			format: 'badge',
			getValue: (a) => (a.archived_at ? 'finished' : a.enabled ? 'on' : 'off'),
			badgeColors: { on: 'badge-success', off: 'badge-muted', finished: 'badge-info' }
		},
		{
			key: 'last_run',
			label: 'Last result',
			format: 'badge',
			getValue: (a) => lastRunStatus(a),
			badgeColors: {
				success: 'badge-success',
				error: 'badge-error',
				skipped: 'badge-muted',
				running: 'badge-warning',
				budget_exceeded: 'badge-warning',
				'—': 'badge-muted'
			}
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
				{ value: 'skipped', label: 'Skipped', badgeColor: 'badge-muted' },
				{ value: 'budget_exceeded', label: 'Over budget', badgeColor: 'badge-warning' }
			],
			predicate: (a, v) => (a.last_run?.status ?? null) === v
		}
	];
</script>

<section class="applets-panel">
	<header class="section-header">
		<div>
			<h2>Applets</h2>
			<p class="subtitle">
				Things that run for you. Ask in chat — "remind me on the 25th,"
				"a dashboard of my heart rate," "write my examen each morning" —
				and it becomes an applet: scheduled, triggered, or always on.
			</p>
		</div>
		<div class="header-applets">
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
			{#if finished.length > 0}
				<button
					type="button"
					class="show-system-btn"
					class:active={showFinished}
					onclick={() => (showFinished = !showFinished)}
					title="Applets whose lifecycle completed — a one-off reminder that fired, or an `until` condition that came true. Their work and their run history are still here."
				>
					{showFinished ? 'Hide' : 'Show'} finished ({finished.length})
				</button>
			{/if}
			<!-- Reconcile is an operator verb — "re-read manifests from disk" is
			     a sentence about the box's internals, and it sat at the top of a
			     consumer page next to New as though it were a thing you do. It
			     lives behind the overflow now: reachable, not offered. -->
			<Popover bind:open={moreMenuOpen} placement="bottom-end" offset={4}>
				{#snippet trigger({ toggle })}
					<button type="button" class="icon-btn" onclick={toggle} aria-label="More">
						<Icon icon="ri:more-2-fill" width="16" />
					</button>
				{/snippet}
				{#snippet children()}
					<div class="new-menu" role="menu">
						<button
							type="button"
							class="new-menu-item"
							role="menuitem"
							disabled={reconciling}
							onclick={() => {
								moreMenuOpen = false;
								void reconcile();
							}}
						>
							<Icon icon="ri:refresh-line" width="16" />
							<div class="new-menu-text">
								<div class="new-menu-title">
									{reconciling ? 'Re-reading…' : 'Re-read from disk'}
								</div>
								<div class="new-menu-desc">
									Pick up applet folders that changed outside the app
								</div>
							</div>
						</button>
					</div>
				{/snippet}
			</Popover>
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
				{#each needsAttention as item (item.applet.id)}
					<button
						type="button"
						class="attention-item"
						onclick={() => openCard(item.applet)}
					>
						{item.applet.name}
						<span class="attention-why">{item.why}</span>
					</button>
				{/each}
			</div>
		</div>
	{/if}

	<!-- The card IS the row the plan specifies: glyph, name, plain-English
	     line, last activity, run-pulse. Defaulting to the table hid every one
	     of those behind a view toggle most people never find, and showed a
	     spreadsheet of cron strings instead. Anyone who prefers the table
	     still has it — dataGridPrefs remembers the choice per entity type. -->
	<UniversalDataGrid
		items={visible}
		{columns}
		{filters}
		entityType="applets"
		defaultViewMode="grid"
		gridMinWidth="340px"
		{loading}
		error={err}
		emptyIcon="ri:flashlight-line"
		emptyMessage="Nothing runs for you yet. Ask in chat — “write my examen each morning,” “remind me on the 25th,” “a dashboard of my heart rate” — and it becomes an applet."
		searchPlaceholder="Search applets…"
		pageSize={50}
		onItemClick={openCard}
		onItemContextMenu={rowContextMenu}
	>
		{#snippet card(applet)}
			<AppletCard
				{applet}
				lastRun={applet.last_run}
				lastSuccessSummary={applet.last_success_summary}
				pulse={applet.pulse ?? []}
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
	/* Matches PageHeading's level-1 title (text-3xl / font-serif / medium) and
	   its description, so a hand-rolled header still reads as a page title. */
	.section-header h2 {
		margin: 0;
		font-family: var(--font-serif, ui-serif, Georgia, serif);
		font-size: 1.875rem;
		line-height: 2.25rem;
		font-weight: 500;
	}
	.subtitle {
		margin: 0.5rem 0 0;
		font-size: 0.875rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.header-applets {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}
	.icon-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 6px;
		background: var(--color-surface, #fff);
		color: var(--color-foreground-subtle, #6b7280);
		cursor: pointer;
	}
	.icon-btn:hover {
		background: var(--color-surface-elevated, #f3f4f6);
		color: var(--color-foreground, #111827);
	}
	.new-menu-item:disabled {
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
		background: var(--hover-bg);
	}
	.attention-why {
		color: var(--color-foreground-subtle);
		font-size: 0.6875rem;
	}
</style>
