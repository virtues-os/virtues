<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Markdown from '$lib/components/Markdown.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import FaceFrame from '$lib/components/applets/FaceFrame.svelte';
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
		getPage,
		type AppletPage,
		runApplet,
		messageApplet,
		type Applet,
		type AppletLogEntry,
		type RunDay
	} from '$lib/api/client';
	import { appletDestination, appletGlyph, describeSchedule, errorHeadline, relativeTime } from '$lib/applets/palette';
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

	// The pages its runs wrote, newest first, and the one open.
	let pages = $state<AppletPage[]>([]);
	let selectedId = $state<string | null>(null);
	let pageContent = $state<string | null>(null);
	const selectedPage = $derived(pages.find((p) => p.page_id === selectedId) ?? pages[0] ?? null);
	$effect(() => {
		const id = selectedPage?.page_id;
		if (!id) return;
		pageContent = null;
		getPage(id)
			.then((p) => {
				if (selectedPage?.page_id === id) pageContent = p.content;
			})
			.catch(() => {
				if (selectedPage?.page_id === id) pageContent = '';
			});
	});

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
	// Where its work goes, under the name, when that says something the page
	// doesn't: an applet whose work is its conversation already has the
	// Conversation button.
	const showsDestination = $derived(
		action ? appletDestination(action) !== '-' && appletGlyph(action) !== 'chats' : false
	);

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
							{status}{#if showsDestination}<span class="goes" aria-label="goes to">
									<Icon icon="ri:arrow-right-line" width="12" />
									{appletDestination(action)}</span
								>{/if}
						</p>
					</div>
					<div class="head-actions">
						{#if chatId}
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

				{#if failedNow}
					<!-- Said once, as one mark and one sentence; the fix is the
					     page's ordinary buttons. -->
					<div class="problem">
						<DayDot state="failed" size="md" />
						<p class="problem-text">
							The last run failed {relativeTime(failedNow.last_at)}.
							{#if failure}{failure.title}. {failure.remedy}{:else if failedNow.error}{errorHeadline(failedNow.error)}{/if}
						</p>
						<div class="problem-actions">
								{#if canRunNow}
									<Button variant="secondary" size="sm" onclick={runNow} disabled={busy}>Retry</Button>
								{/if}
								{#if chatId}
									<Button variant="secondary" size="sm" onclick={openConversation}>Ask why</Button>
								{/if}
						</div>
					</div>
				{:else if overdue}
					<div class="problem">
						<DayDot state="missed" size="md" />
						<p class="problem-text">It was due {relativeTime(action.next_due_at)} and hasn't run.</p>
						<div class="problem-actions">
								{#if canRunNow}
									<Button variant="secondary" size="sm" onclick={runNow} disabled={busy}>Run now</Button>
								{/if}
						</div>
					</div>
				{/if}

				{#if nextRun}
					<p class="next">{nextRun}</p>
				{/if}

				<!-- The face IS the home when there is one. -->
				{#if action.has_face}
					<FaceFrame appletId={action.id} height="460px" />
					<ShareSheet
						open={sharing}
						producer={{ kind: 'applet', id: action.id }}
						onClose={() => (sharing = false)}
					/>
				{:else if pages.length > 0}
					<!-- What it made: the newest page open, every page beside it. -->
					<div class="pages">
						<article class="page">
							{#if selectedPage}
								<h2 class="page-title">{selectedPage.title}</h2>
								<p class="page-by">
									Written by {action.name}, {when(selectedPage.written_at)}
									<Button variant="ghost" size="sm" onclick={() => openPage(selectedPage.page_id)}>Open page</Button>
								</p>
								{#if pageContent === null}
									<p class="empty">Loading…</p>
								{:else}
									<div class="page-body"><Markdown content={pageContent} variant="article" /></div>
								{/if}
							{/if}
						</article>
						<nav class="page-list" aria-label={`Pages ${action.name} wrote`}>
							<h2 class="list-head">All pages</h2>
							{#each pages as p (p.page_id)}
								<button
									type="button"
									class="page-item"
									aria-current={p.page_id === selectedPage?.page_id}
									onclick={() => (selectedId = p.page_id)}
								>
									<span class="page-item-title">{p.title}</span>
									<span class="page-item-when">{when(p.written_at)}</span>
								</button>
							{/each}
						</nav>
					</div>
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
		align-items: center;
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
	.goes {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin-left: 8px;
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

	/* A status surface: a neutral wash, one mark, one sentence, and the
	   page's ordinary buttons as the fix (design-grammar §5). */
	.problem {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px 12px;
		padding: 12px 16px;
		border-radius: 12px;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}
	.problem-text {
		flex: 1;
		min-width: 240px;
		margin: 0;
		font-size: 15px;
		line-height: 1.5;
	}
	.problem-actions {
		display: flex;
		align-items: center;
		gap: 8px;
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

	/* The page it made, with every page it made beside it. */
	.pages {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 240px;
		gap: 32px;
		align-items: start;
	}
	@container (max-width: 720px) {
		.pages {
			grid-template-columns: 1fr;
		}
	}
	.page-title {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 36px;
		line-height: 1.15;
	}
	.page-by {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 8px 0 16px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.page-body {
		max-width: 40em;
	}
	.page-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.page-item {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 12px;
		border: 0;
		border-radius: 6px;
		background: none;
		text-align: left;
		font: inherit;
		cursor: pointer;
		color: var(--color-foreground);
	}
	.page-item:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}
	.page-item[aria-current='true'] {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
	}
	.page-item-title {
		font-family: var(--font-serif);
		font-size: 16px;
	}
	.page-item-when {
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	/* Info sits at a reading width under what the applet made. */
	.info-section {
		max-width: 640px;
	}
	.empty {
		margin: 0;
		font-size: 15px;
		color: var(--color-foreground-muted);
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
