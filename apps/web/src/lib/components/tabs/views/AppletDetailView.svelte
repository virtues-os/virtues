<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Markdown from '$lib/components/Markdown.svelte';
	import FaceFrame from '$lib/components/applets/FaceFrame.svelte';
	import Card from '$lib/components/Card.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import ShareSheet from '$lib/components/applets/ShareSheet.svelte';
	import DayDot from '$lib/components/applets/DayDot.svelte';
	import AppletInfo from '$lib/components/applets/AppletInfo.svelte';
	import DeleteAppletModal from '$lib/components/applets/DeleteAppletModal.svelte';
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
		patchApplet,
		type Applet,
		type AppletLogEntry,
		type RunDay
	} from '$lib/api/client';
	import { appletDestination, appletGlyph, describeSchedule, errorHeadline, relativeTime } from '$lib/applets/palette';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { indexRunDays } from '$lib/applets/days';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { explainRunError } from '$lib/sources/run-errors';

	/**
	 * An applet's home opens on its results: its newest page with every page
	 * beside it, its dashboard live, or what its last runs did. The frame
	 * around them is the same for every applet: name, schedule and where the
	 * work goes, the switch, Conversation, Info and More.
	 *
	 * Info (`?panel=info`) holds Run history and Technical details, each
	 * opening in place. `?panel=history` and `?panel=details` open Info with
	 * that one open, which is where the table's dots and Last run link.
	 */
	let { tab }: { tab: Tab; active: boolean } = $props();

	const url = $derived(new URL(tab.route, 'http://localhost'));
	const appletId = $derived(url.pathname.match(/^\/(?:applet|action)\/(applet_[^/]+)$/)?.[1] ?? null);
	const asked = $derived(url.searchParams.get('panel'));
	const inInfo = $derived(asked === 'info' || asked === 'history' || asked === 'details');

	function showInfo(open: boolean) {
		if (!appletId) return;
		const base = `/applet/${appletId}`;
		windowShellStore.updateTab(tab.id, { route: open ? `${base}?panel=info` : base });
	}

	let historyOpen = $state(false);
	let technicalOpen = $state(false);
	$effect(() => {
		if (asked === 'history') historyOpen = true;
		if (asked === 'details') technicalOpen = true;
	});

	let action = $state<Applet | null>(null);
	let log = $state<AppletLogEntry[]>([]);
	let runDays = $state<RunDay[]>([]);
	let daysErr = $state<string | null>(null);
	let loading = $state(false);
	let busy = $state(false);
	let err = $state<string | null>(null);
	let sharing = $state(false);
	let deleting = $state(false);

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

	// Where its work goes, by the same rule as the icon and the table's Goes
	// to, so the three can't disagree.
	const made = $derived.by((): 'pages' | 'dashboard' | 'chat' | null => {
		const glyph = action ? appletGlyph(action) : null;
		if (glyph === 'pages') return 'pages';
		if (glyph === 'dashboard') return 'dashboard';
		if (glyph === 'chats') return 'chat';
		return null;
	});
	// The newest run that said something: a chat applet's latest result.
	const lastSaid = $derived(log.find((e) => e.status === 'success' && e.summary) ?? null);

	const STATUS_WORD: Record<AppletLogEntry['status'], string> = {
		success: 'Ran',
		error: 'Failed',
		running: 'Running',
		skipped: 'Skipped',
		cancelled: 'Cancelled',
		budget_exceeded: 'Stopped at its spending limit'
	};

	/** One run in a line: what it said, or why it failed. */
	function runLine(e: AppletLogEntry): string {
		const what =
			e.status === 'error'
				? `Failed${e.error ? `: ${errorHeadline(e.error, 120)}` : ''}`
				: (e.summary ?? STATUS_WORD[e.status]);
		return e.occurrences > 1 ? `${what} (${e.occurrences} times)` : what;
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
			const [l, d, p] = await Promise.allSettled([getAppletLog(id), getRunsByDay(49), getAppletPages(id)]);
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

	// The switch is the one thing every applet's page offers, so it sits in
	// the header rather than in a card below the work.
	let saving = $state(false);
	async function setEnabled(enabled: boolean) {
		if (!action) return;
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

	const status = $derived.by(() => {
		if (!action) return '';
		if (action.archived_at) return `Finished ${new Date(action.archived_at).toLocaleDateString()}`;
		// Off keeps its schedule in view: it's what turning it back on brings back.
		if (!action.enabled) return `Off · ${describeSchedule(action.schedule)}`;
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
		if (a.owner !== 'system') {
			items.push({
				id: 'delete',
				label: 'Delete applet',
				icon: 'ri:delete-bin-line',
				variant: 'destructive',
				dividerBefore: true,
				action: () => {
					deleting = true;
				}
			});
		}
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
			{#if inInfo}
				<div class="narrow">
					<header class="head sub">
						<button type="button" class="backlink" onclick={() => showInfo(false)}>
							<Icon icon="ri:arrow-left-line" width="14" />
							{action.name}
						</button>
						<h1 class="title">Info</h1>
						{#if action.description}<p class="status">{action.description}</p>{/if}
					</header>
					<AppletInfo bind:action {index} {daysErr} bind:historyOpen bind:technicalOpen>
						{#snippet history()}
							{#if action}<RunHistory {action} {log} {index} {daysErr} {collectorDenied} />{/if}
						{/snippet}
						{#snippet technical()}
							{#if action}
								<AppletSettings
									bind:action
									onRenamed={(name) => windowShellStore.updateTab(tab.id, { label: name })}
								/>
							{/if}
						{/snippet}
					</AppletInfo>
				</div>
			{:else}
				<header class="head">
					<span class="glyph" aria-hidden="true"><AtlasIcon name={appletGlyph(action)} size={20} bare /></span>
					<div class="title-block">
						<h1 class="title">{action.name}</h1>
						<p class="status">
							{status}{#if appletDestination(action) !== '-' && made !== 'chat'}<span class="goes" aria-label="goes to">
									<Icon icon="ri:arrow-right-line" width="12" />
									{appletDestination(action)}</span
								>{/if}
						</p>
					</div>
					<div class="head-actions">
						{#if action.archived_at}
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
								onclick={() => setEnabled(!action!.enabled)}
							></button>
						{/if}
						{#if chatId}
							<Button variant="secondary" size="sm" icon="ri:chat-3-line" onclick={openConversation}>
								Conversation
							</Button>
						{/if}
						<Button variant="secondary" size="sm" icon="ri:information-line" onclick={() => showInfo(true)}>
							Info
						</Button>
						<IconButton icon="ri:more-line" label="More" variant="secondary" haspopup="menu" onclick={openMore} />
					</div>
				</header>

				{#if err}
					<p class="error-msg">{err}</p>
				{/if}

				<!-- What needs you, under the header and above the results: one
				     row, with its fix beside it. -->
				{#if failedNow || overdue}
					<Card list>
						<div class="needs">
							<DayDot state={failedNow ? 'failed' : 'missed'} size="md" />
							<span class="needs-text">
								{#if failedNow}
									<span class="needs-title">The last run failed {relativeTime(failedNow.last_at)}</span>
									{#if failure}
										<span class="needs-sub">{failure.title}. {failure.remedy}</span>
									{:else if failedNow.error}
										<span class="needs-sub">{errorHeadline(failedNow.error)}</span>
									{/if}
								{:else}
									<span class="needs-title">It was due {relativeTime(action.next_due_at)} and hasn't run</span>
								{/if}
							</span>
							{#if canRunNow}
								<Button variant="secondary" size="sm" onclick={runNow} disabled={busy}>
									{failedNow ? 'Retry' : 'Run now'}
								</Button>
							{/if}
						</div>
					</Card>
				{/if}

				<!-- Its results. They look different from applet to applet
				     because the work does; the frame above stays the same. -->
				{#if made === 'dashboard' && action.has_face}
					<FaceFrame appletId={action.id} height="460px" />
				{:else if made === 'dashboard'}
					<p class="empty">No dashboard yet. It's set to make one and hasn't yet.</p>
				{:else if made === 'pages' && selectedPage}
					<div class="pages">
						<article class="page">
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
						</article>
						<nav class="page-list" aria-label={`Pages ${action.name} wrote`}>
							<h2 class="list-head">All pages</h2>
							{#each pages as p (p.page_id)}
								<button
									type="button"
									class="page-item"
									aria-current={p.page_id === selectedPage.page_id}
									onclick={() => (selectedId = p.page_id)}
								>
									<span class="page-item-title">{p.title}</span>
									<span class="page-item-when">{when(p.written_at)}</span>
								</button>
							{/each}
						</nav>
					</div>
				{:else if made === 'pages'}
					<p class="empty">No pages yet. Its pages show here after it writes one.</p>
				{:else if made === 'chat' && lastSaid}
					<section>
						<h2 class="list-head">Latest{#if lastSaid.last_at}{' · '}{when(lastSaid.last_at)}{/if}</h2>
						<Card list><p class="said">{lastSaid.summary}</p></Card>
					</section>
				{:else if log.length > 0}
					<!-- Nothing to show but what its runs did: one line each,
					     failures included. -->
					<section>
						<h2 class="list-head">Recent runs</h2>
						<ul class="recent-list" role="list">
							{#each log.slice(0, 7) as e (e.run_id ?? e.last_at)}
								<li class="recent-item">
									<DayDot
										state={e.status === 'error'
											? 'failed'
											: e.status === 'budget_exceeded'
												? 'stopped'
												: e.status === 'success'
													? 'ran'
													: 'quiet'}
									/>
									<span class="recent-when">{e.last_at ? dayLabel(e.last_at) : ''}</span>
									<span class="recent-what" class:failed={e.status === 'error'}>{runLine(e)}</span>
								</li>
							{/each}
						</ul>
					</section>
				{:else}
					<p class="empty">
						{action.enabled ? 'Nothing yet. Its results show here after it runs.' : "It's off, so it hasn't run yet."}
					</p>
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
			{/if}
		</div>
		<DeleteAppletModal {action} bind:open={deleting} onDeleted={() => windowShellStore.closeTab(tab.id)} />
		{#if action.has_face}
			<ShareSheet open={sharing} producer={{ kind: 'applet', id: action.id }} onClose={() => (sharing = false)} />
		{/if}
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

	.list-head {
		margin: 0 0 8px;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}

	.goes {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin-left: 8px;
	}

	/* Info: one narrow column, like a sheet laid on the applet. */
	.narrow {
		width: 100%;
		max-width: 640px;
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
		flex-direction: column;
		gap: 4px;
		align-items: flex-start;
	}

	.needs {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 48px;
		padding: 8px 16px;
	}
	.needs-text {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.needs-title {
		font-size: 14px;
		font-weight: 500;
	}
	.needs-sub {
		font-size: 13px;
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

	.said {
		margin: 0;
		padding: 16px;
		font-size: 15px;
		line-height: 1.5;
	}

	/* What its last runs did, one line each. */
	.recent-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.recent-item {
		display: grid;
		grid-template-columns: 8px 120px minmax(0, 1fr);
		align-items: center;
		gap: 12px;
		min-height: 40px;
		border-bottom: 1px solid var(--color-border);
		font-size: 14px;
	}
	.recent-when {
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.recent-what {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.recent-what.failed {
		color: var(--color-error);
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
