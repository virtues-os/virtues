<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Card from '$lib/components/Card.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import ShareSheet from '$lib/components/applets/ShareSheet.svelte';
	import DayDot from '$lib/components/applets/DayDot.svelte';
	import AppletInfo from '$lib/components/applets/AppletInfo.svelte';
	import AppletSettings from '$lib/components/applets/AppletSettings.svelte';
	import RunHistory from '$lib/components/applets/RunHistory.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { pinMenuItem } from '$lib/pins/pinAction';
	import type { Tab } from '$lib/tabs/types';
	import {
		getApplet,
		getAppletLog,
		getRunsByDay,
		getAppletPages,
		type AppletPage,
		runApplet,
		messageApplet,
		type Applet,
		type AppletLogEntry,
		type RunDay
	} from '$lib/api/client';
	import { appletGlyph, describeSchedule, errorHeadline, relativeTime } from '$lib/applets/palette';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { indexRunDays } from '$lib/applets/days';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { explainRunError } from '$lib/sources/run-errors';

	/**
	 * An applet's home: what it made, first. Info, Run history and Technical
	 * details open in place, named by `?panel=` in the route so the table's
	 * dots and Last run cell can link straight to Run history.
	 */
	let { tab }: { tab: Tab; active: boolean } = $props();

	// The applet's page carries its Info; Run history and Technical details
	// open under it, and their Back returns to it.
	type Panel = 'home' | 'history' | 'details';
	const PANEL_TITLE: Record<Exclude<Panel, 'home'>, string> = {
		history: 'Run history',
		details: 'Technical details'
	};

	const url = $derived(new URL(tab.route, 'http://localhost'));
	const appletId = $derived(url.pathname.match(/^\/(?:applet|action)\/(applet_[^/]+)$/)?.[1] ?? null);
	const panel = $derived.by((): Panel => {
		const p = url.searchParams.get('panel');
		// `?panel=info` was Info's own panel; it is the page now.
		return p === 'history' || p === 'details' ? p : 'home';
	});

	function go(p: Panel) {
		if (!appletId) return;
		const base = `/applet/${appletId}`;
		windowShellStore.updateTab(tab.id, { route: p === 'home' ? base : `${base}?panel=${p}` });
	}

	let action = $state<Applet | null>(null);
	let log = $state<AppletLogEntry[]>([]);
	let runDays = $state<RunDay[]>([]);
	let daysErr = $state<string | null>(null);
	let loading = $state(false);
	let busy = $state(false);
	let err = $state<string | null>(null);
	let sharing = $state(false);

	const index = $derived(indexRunDays(runDays));

	// The pages its runs wrote, newest first.
	let pages = $state<AppletPage[]>([]);
	const PAGES_SHOWN = 5;
	let allPages = $state(false);
	const shownPages = $derived(allPages ? pages : pages.slice(0, PAGES_SHOWN));

	// What the "What it made" card points to: its pages, its dashboard, or
	// its conversation when that is where its work goes. Nothing otherwise.
	const made = $derived.by((): 'pages' | 'dashboard' | 'chat' | null => {
		if (!action) return null;
		if (pages.length > 0) return 'pages';
		if (action.has_face) return 'dashboard';
		if (chatId && appletGlyph(action) === 'chats') return 'chat';
		return null;
	});
	// The last thing it said, under "Its conversation".
	const lastSaid = $derived(log.find((e) => e.status === 'success' && e.summary)?.summary ?? null);

	function openDashboard() {
		if (action) windowShellStore.openTabFromRoute(`/applet/${action.id}/view`);
	}

	function openPage(id: string) {
		windowShellStore.openTabFromRoute(`/page/${id}`, { focusExisting: true });
	}

	/** "Today at 7:00", "Yesterday at 7:00", "Mon 5 Oct at 7:00". */
	function when(iso: string): string {
		const d = new Date(iso);
		const time = d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
		return `${dayLabel(iso)} at ${time}`;
	}

	function dayLabel(iso: string): string {
		const d = new Date(iso);
		const today = new Date();
		const yesterday = new Date(today);
		yesterday.setDate(today.getDate() - 1);
		if (d.toDateString() === today.toDateString()) return 'Today';
		if (d.toDateString() === yesterday.toDateString()) return 'Yesterday';
		return d.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' });
	}


	$effect(() => {
		if (appletId) void load(appletId);
	});

	async function load(id: string) {
		loading = true;
		err = null;
		try {
			// The applet loads on its own. The log and the day counts are
			// weaker requests: one of them failing must not blank the page.
			const a = await getApplet(id);
			action = a;
			windowShellStore.updateTab(tab.id, { label: a.name });
			const [l, d, p] = await Promise.allSettled([getAppletLog(id), getRunsByDay(35), getAppletPages(id)]);
			log = l.status === 'fulfilled' ? l.value : [];
			pages = p.status === 'fulfilled' ? p.value : [];
			if (d.status === 'fulfilled') {
				runDays = d.value.filter((r) => r.applet_id === id);
				daysErr = null;
			} else {
				daysErr = d.reason instanceof Error ? d.reason.message : String(d.reason);
			}
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	const chatId = $derived.by(() => {
		const id = action?.config?.chat_id;
		return typeof id === 'string' && id ? id : null;
	});

	function openConversation() {
		if (!chatId || !action) return;
		windowShellStore.openAside({
			type: 'chat',
			label: action.name,
			route: `/chat/${chatId}`,
			icon: 'atlas:applets'
		});
	}

	// "Run now" fires `trigger = "manual"`, which the runner refuses unless the
	// applet lists it, so it's offered only where it can work.
	const canRunNow = $derived(
		Boolean(action?.triggers?.includes('manual')) && Boolean(action?.enabled) && !action?.archived_at
	);

	// The collector's current self-report, for pinning a failed run's bare
	// "Permission denied" to a named macOS permission (sources/run-errors.ts).
	$effect(() => {
		if (action?.device_id) void sourcesStore.load();
	});
	const collectorDenied = $derived(
		action?.device_id
			? (sourcesStore.connections.find((c) => c.id === action?.device_id)?.denied ?? [])
			: []
	);

	// A run is detached: the POST returns as soon as the row exists and the
	// agent keeps going. Poll while anything is running, and stop the moment
	// nothing is. Bounded so a wedged run can't leave a timer chain going.
	const POLL_MS = 1500;
	const POLL_MAX = 40; // ~60s, past which a run is not "just finishing"
	let pollTimer: ReturnType<typeof setTimeout> | null = null;
	let pollsLeft = 0;

	function stopPolling() {
		if (pollTimer) clearTimeout(pollTimer);
		pollTimer = null;
		pollsLeft = 0;
	}

	function pollSoon() {
		if (pollTimer) clearTimeout(pollTimer);
		if (pollsLeft <= 0) return;
		pollTimer = setTimeout(async () => {
			pollsLeft -= 1;
			const id = action?.id;
			if (!id) return;
			log = await getAppletLog(id).catch(() => log);
			if (log.some((e) => e.status === 'running')) {
				pollSoon();
			} else {
				stopPolling();
				// The run may have finished the applet, or spent something.
				await load(id);
			}
		}, POLL_MS);
	}

	function watchForResult() {
		pollsLeft = POLL_MAX;
		pollSoon();
	}

	// Leaving the page, or switching applets, must not leave a chain running.
	$effect(() => {
		void appletId;
		return stopPolling;
	});

	async function runNow() {
		if (!action) return;
		busy = true;
		err = null;
		try {
			await runApplet(action.id);
			log = await getAppletLog(action.id).catch(() => log);
			watchForResult();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	// The message wake: a composer on the home, so "I had eggs" goes to the
	// applet and "make it weekly" goes to its prompt, and the two can't be
	// confused.
	let draft = $state('');
	let sending = $state(false);
	const canMessage = $derived(Boolean(action?.triggers?.includes('message')) && !action?.archived_at);
	// Messaging an applet that's off reaches `prepare_run`, which reports it
	// as not found, so the composer waits for the switch.
	const canSend = $derived(canMessage && Boolean(action?.enabled));

	async function send() {
		const text = draft.trim();
		if (!action || !text) return;
		sending = true;
		err = null;
		try {
			await messageApplet(action.id, text);
			draft = '';
			log = await getAppletLog(action.id).catch(() => log);
			watchForResult();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			sending = false;
		}
	}

	// The newest run that said something, for the home's body when the applet
	// has no face. The log is newest first.
	const lastEntry = $derived(log[0] ?? null);
	const failedNow = $derived(lastEntry?.status === 'error' ? lastEntry : null);
	const failure = $derived(failedNow?.error ? explainRunError(failedNow.error, collectorDenied) : null);

	// Same grace the scheduler and the table use.
	const OVERDUE_GRACE_MS = 60 * 60 * 1000;

	const overdue = $derived(
		Boolean(
			action?.next_due_at &&
				action.enabled &&
				!action.archived_at &&
				Date.now() - new Date(action.next_due_at).getTime() > OVERDUE_GRACE_MS
		)
	);

	// When it runs next, so a failure reads as "today failed, tomorrow is
	// scheduled" rather than as a dead end.
	const nextRun = $derived(
		action?.next_due_at && action.enabled && !action.archived_at && !overdue
			? `Next run ${relativeTime(action.next_due_at)}`
			: null
	);

	const status = $derived.by(() => {
		if (!action) return '';
		if (action.archived_at) return `Finished ${new Date(action.archived_at).toLocaleDateString()}`;
		if (!action.enabled) return 'Off';
		return describeSchedule(action.schedule);
	});

	function openMore(e: MouseEvent) {
		if (!action) return;
		const a = action;
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const items: ContextMenuItem[] = [];
		if (canRunNow) {
			items.push({ id: 'run', label: 'Run now', icon: 'ri:play-line', action: () => void runNow() });
		}
		if (a.has_face) {
			items.push({
				id: 'share',
				label: 'Share',
				icon: 'ri:share-forward-line',
				action: () => {
					sharing = true;
				}
			});
			items.push({
				id: 'full',
				label: 'Open full page',
				icon: 'ri:external-link-line',
				action: () => {
					windowShellStore.openTabFromRoute(`/applet/${a.id}/view`);
				}
			});
		}
		items.push(
			pinMenuItem({ url: `/applet/${a.id}`, label: a.name, icon: 'atlas:applets' }, { dividerBefore: items.length > 0 })
		);
		contextMenu.show({ x: rect.left, y: rect.bottom + 4 }, items);
	}
</script>

<div class="applet-home">
	{#if loading && !action}
		<p class="state">Loading…</p>
	{:else if err && !action}
		<p class="state error-msg">{err}</p>
	{:else if action}
		<div class="measure">
			{#if panel === 'home'}
				<header class="head">
					<span class="glyph" aria-hidden="true"><AtlasIcon name={appletGlyph(action)} size={20} bare /></span>
					<div class="title-block">
						<h1 class="title">{action.name}</h1>
						<p class="status">
							{status}
						</p>
						{#if failedNow}
							<!-- A fact about this applet, under its name: one mark and
							     one sentence. Its fix (Retry) sits with the header's
							     buttons, so the line stays as tight as the one above it. -->
							<p class="problem">
								<DayDot state="failed" />
								<span>
									The last run failed {relativeTime(failedNow.last_at)}.
									{#if failure}{failure.title}. {failure.remedy}{:else if failedNow.error}{errorHeadline(failedNow.error)}{/if}
								</span>
							</p>
						{:else if overdue}
							<p class="problem">
								<DayDot state="missed" />
								<span>It was due {relativeTime(action.next_due_at)} and hasn't run.</span>
							</p>
						{/if}
					</div>
					<div class="head-actions">
						{#if (failedNow || overdue) && canRunNow}
							<Button variant="secondary" size="sm" onclick={runNow} disabled={busy}>
								{failedNow ? 'Retry' : 'Run now'}
							</Button>
						{/if}
						{#if chatId && made !== 'chat'}
							<Button variant="secondary" size="sm" icon="ri:chat-3-line" onclick={openConversation}>
								Conversation
							</Button>
						{/if}
						<IconButton icon="ri:more-line" label="More" variant="secondary" haspopup="menu" onclick={openMore} />
					</div>
				</header>

				{#if action.description}
					<p class="desc">{action.description}</p>
				{/if}

				{#if err}
					<p class="error-msg">{err}</p>
				{/if}

				{#if nextRun}
					<p class="next">{nextRun}</p>
				{/if}

				<!-- What it made, as one card for every applet: a way to where the
				     work lives (Pages, its chat, its dashboard), not a copy of it. -->
				{#if made}
					<section class="made">
						<h2 class="list-head">What it made</h2>
						<Card list>
							{#if made === 'pages'}
								{#each shownPages as p (p.page_id)}
									<button type="button" class="made-row" onclick={() => openPage(p.page_id)}>
										<AtlasIcon name="pages" size={16} bare />
										<span class="made-text">
											<span class="made-title">{p.title}</span>
											<span class="made-sub">{when(p.written_at)}</span>
										</span>
										<Icon icon="ri:arrow-right-s-line" width="16" />
									</button>
								{/each}
								{#if pages.length > PAGES_SHOWN}
									<button type="button" class="made-row more" onclick={() => (allPages = !allPages)}>
										<span class="made-text">{allPages ? 'Show fewer' : `Show all ${pages.length} pages`}</span>
									</button>
								{/if}
							{:else if made === 'dashboard'}
								<button type="button" class="made-row" onclick={openDashboard}>
									<AtlasIcon name="dashboard" size={16} bare />
									<span class="made-text">
										<span class="made-title">Its dashboard</span>
										<span class="made-sub">Live, opens full size</span>
									</span>
									<Icon icon="ri:arrow-right-s-line" width="16" />
								</button>
							{:else}
								<button type="button" class="made-row" onclick={openConversation}>
									<AtlasIcon name="chats" size={16} bare />
									<span class="made-text">
										<span class="made-title">Its conversation</span>
										{#if lastSaid}<span class="made-sub">{lastSaid}</span>{/if}
									</span>
									<Icon icon="ri:arrow-right-s-line" width="16" />
								</button>
							{/if}
						</Card>
					</section>
				{/if}
				{#if action.has_face}
					<ShareSheet
						open={sharing}
						producer={{ kind: 'applet', id: action.id }}
						onClose={() => (sharing = false)}
					/>
				{/if}

				{#if canMessage}
					<form
						class="composer"
						onsubmit={(e) => {
							e.preventDefault();
							void send();
						}}
					>
						<input
							type="text"
							bind:value={draft}
							disabled={sending || !canSend}
							placeholder={canSend ? `Tell ${action.name} something…` : 'Turn it on to send it anything'}
						/>
						<Button variant="primary" onclick={send} disabled={sending || !canSend || !draft.trim()}>
							{sending ? 'Sending…' : 'Send'}
						</Button>
					</form>
				{/if}

				<!-- Its Info, on its page: how its week went, the switch, when it
				     runs, where its work goes, what it costs. -->
				<section class="info-section" aria-label={`About ${action.name}`}>
					<AppletInfo
						bind:action
						{index}
						{daysErr}
						onHistory={() => go('history')}
						onTechnical={() => go('details')}
						onDeleted={() => windowShellStore.closeTab(tab.id)}
					/>
				</section>
			{:else}
				<div class="narrow">
				<header class="head sub">
					<button type="button" class="backlink" onclick={() => go('home')}>
						<Icon icon="ri:arrow-left-line" width="14" />
						{action.name}
					</button>
					<h1 class="title">{PANEL_TITLE[panel]}</h1>
					<p class="status">
						{panel === 'history' ? `${action.name} · ${describeSchedule(action.schedule)}` : action.name}
					</p>
				</header>

				{#if panel === 'history'}
					<RunHistory
						{action}
						{log}
						{index}
						{daysErr}
						{collectorDenied}
						{busy}
						onRetry={canRunNow ? runNow : undefined}
						onAsk={chatId ? openConversation : undefined}
					/>
				{:else}
					<AppletSettings bind:action onRenamed={(name) => windowShellStore.updateTab(tab.id, { label: name })} />
				{/if}
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.applet-home {
		height: 100%;
		overflow-y: auto;
		padding: 40px 48px 64px;
		background: var(--color-surface);
	}
	@container (max-width: 640px) {
		.applet-home {
			padding: 24px 16px 48px;
		}
	}
	.measure {
		max-width: 920px;
		margin: 0 auto;
		display: flex;
		flex-direction: column;
		gap: 24px;
	}
	.state {
		text-align: center;
		color: var(--color-foreground-subtle);
		font-size: 14px;
	}

	.head {
		display: flex;
		align-items: flex-start;
		gap: 16px;
		padding-bottom: 16px;
		border-bottom: 1px solid var(--color-border);
	}
	.glyph {
		width: 40px;
		height: 40px;
		flex: none;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 12px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground-muted);
	}
	/* Info and the pages under it are one narrow column, like a sheet laid
	   on the applet's page. */
	.narrow {
		width: 100%;
		max-width: 560px;
		margin: 0 auto;
		display: flex;
		flex-direction: column;
		gap: 24px;
	}
	.backlink {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 16px;
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
	.head.sub {
		padding-bottom: 0;
		border-bottom: 0;
		flex-direction: column;
		gap: 4px;
		align-items: flex-start;
	}
	.title-block {
		flex: 1;
		min-width: 0;
	}
	.title {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--md-h2-size);
		line-height: 1.25;
	}
	.status {
		margin: 4px 0 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.head-actions {
		display: flex;
		align-items: center;
		gap: 8px;
		flex: none;
	}

	.problem {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 4px 0 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground);
	}

	.desc {
		margin: -8px 0 0;
		max-width: 40em;
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.next {
		margin: -8px 0 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.list-head {
		margin: 0 0 8px;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}

	/* What it made: one card, the same for every applet. */
	.made {
		max-width: 640px;
	}
	.made-row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		min-height: 48px;
		padding: 8px 16px;
		border: 0;
		background: none;
		font: inherit;
		text-align: left;
		cursor: pointer;
		color: var(--color-foreground);
	}
	.made-row + .made-row {
		border-top: 1px solid var(--color-border);
	}
	.made-row:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}
	.made-row.more .made-text {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.made-text {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.made-title {
		font-family: var(--font-serif);
		font-size: 16px;
	}
	.made-sub {
		font-size: 13px;
		color: var(--color-foreground-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	/* Info sits at a reading width under what the applet made. */
	.info-section {
		max-width: 640px;
	}

	.composer {
		display: flex;
		gap: 8px;
	}
	.composer input {
		flex: 1;
		min-width: 0;
		font: inherit;
		font-size: 14px;
		padding: 8px 12px;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
		color: var(--color-foreground);
	}
	.error-msg {
		margin: 0;
		font-size: 13px;
		color: var(--color-error);
	}
</style>
