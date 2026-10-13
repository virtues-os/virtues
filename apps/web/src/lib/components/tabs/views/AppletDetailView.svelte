<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Markdown from '$lib/components/Markdown.svelte';
	import FaceFrame from '$lib/components/applets/FaceFrame.svelte';
	import Card from '$lib/components/Card.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import ShareSheet from '$lib/components/applets/ShareSheet.svelte';
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
	import { appletGlyph, describeSchedule, errorHeadline, relativeTime, runFix } from '$lib/applets/palette';
	import { formatMicrosPrecise } from '$lib/utils/currency';
	import RecentDots from '$lib/components/applets/RecentDots.svelte';
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

	/** Info's full view, from the inspector, with Run history or Technical
	 *  details already open. */
	function openInfo(panel: 'history' | 'details') {
		if (appletId) windowShellStore.updateTab(tab.id, { route: `/applet/${appletId}?panel=${panel}` });
	}

	// The inspector: Info beside the results, so they stay in view. Closed
	// whenever the tab moves to another applet.
	let insp = $state(false);
	$effect(() => {
		void appletId;
		insp = false;
	});

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

	// What went wrong, said once, where the missing result would be.
	const problem = $derived(Boolean(failedNow || overdue));
	const problemText = $derived(
		failedNow
			? failure
				? `${failure.title}. ${failure.remedy}`
				: failedNow.error
					? errorHeadline(failedNow.error)
					: 'The run failed.'
			: overdue && action?.next_due_at
				? `It was due ${relativeTime(action.next_due_at)} and hasn't run.`
				: null
	);
	const missedWhen = $derived(failedNow?.last_at ?? (overdue ? (action?.next_due_at ?? null) : null));
	// Reconnect when the sign-in lapsed; otherwise another try is the fix.
	const fix = $derived(failedNow ? runFix(failedNow.error ?? null) : null);
	function openFix() {
		if (fix) windowShellStore.openTabFromRoute(fix.href, { focusExisting: true });
	}

	/** "Tuesday, 6 October". */
	function longDay(iso: string): string {
		return new Date(iso).toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' });
	}

	// A list of what its runs produced: a conversation applet's results, or
	// for one that makes nothing to show, what each run did. The failure the
	// row above names isn't repeated.
	const results = $derived(
		made === 'chat'
			? log.filter((e) => e.status === 'success' && e.summary).slice(0, 8)
			: log.filter((e) => e !== failedNow).slice(0, 7)
	);

	const nextRunText = $derived.by(() => {
		if (!action || action.archived_at) return 'Finished';
		if (!action.enabled) return 'Off';
		if (!action.next_due_at) return describeSchedule(action.schedule);
		return relativeTime(action.next_due_at);
	});
	// The week's cost comes with the applet list, not with one applet.
	const costText = $derived.by(() => {
		if (!appletsStore.loaded) return '…';
		const m = appletsStore.list.find((a) => a.id === action?.id)?.spend_week_micros;
		// null means the server couldn't read it, which is not the same as free.
		if (m === null || m === undefined) return "Couldn't read it";
		if (m === 0) return 'Nothing this week';
		if (m < 100) return 'Under a cent this week';
		return `${formatMicrosPrecise(m)} this week`;
	});

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

{#snippet fixAction()}
	{#if fix}
		<TextAction inline onclick={openFix}>{fix.label}</TextAction>
	{:else if canRunNow}
		<TextAction inline onclick={runNow} disabled={busy}>{failedNow ? 'Retry' : 'Run now'}</TextAction>
	{/if}
{/snippet}

{#snippet problemRow()}
	<li class="result missing">
		<span class="result-text">
			{#if missedWhen}<span class="result-when">{longDay(missedWhen)}</span>{/if}
			<span class="failed">{problemText}</span>
		</span>
		{#if fix}
			<Button variant="primary" size="sm" onclick={openFix}>{fix.label}</Button>
		{:else if canRunNow}
			<TextAction onclick={runNow} disabled={busy}>{failedNow ? 'Retry' : 'Run now'}</TextAction>
		{/if}
	</li>
{/snippet}

{#snippet inspector()}
	{#if action}
		<aside class="insp" aria-label={`Info for ${action.name}`}>
			{#if action.description}
				<section>
					<h2 class="list-head">Goal</h2>
					<p class="insp-text">{action.description}</p>
				</section>
			{/if}
			<section>
				<h2 class="list-head">Recent runs</h2>
				{#if daysErr}
					<p class="insp-text muted">Your server couldn't read its recent runs.</p>
				{:else}
					<RecentDots {action} {index} />
				{/if}
			</section>
			<Card list>
				<div class="insp-row"><span>Next run</span><span class="insp-value">{nextRunText}</span></div>
				<div class="insp-row"><span>Cost</span><span class="insp-value">{costText}</span></div>
				<button type="button" class="insp-row link" onclick={() => openInfo('history')}>
					<span>Run history</span><Icon icon="ri:arrow-right-s-line" width="16" />
				</button>
				<button type="button" class="insp-row link" onclick={() => openInfo('details')}>
					<span>Technical details</span><Icon icon="ri:arrow-right-s-line" width="16" />
				</button>
			</Card>
		</aside>
	{/if}
{/snippet}

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
						<p class="status">{status}</p>
					</div>
					<!-- One title block, the icons centred on it, the switch last. -->
					<div class="head-actions">
						{#if chatId}
							<!-- A word with the icon: a speech bubble alone didn't say you
							     can tell it things or change it there. -->
							<Button variant="ghost" size="sm" icon="ri:chat-3-line" onclick={openConversation}>Chat</Button>
						{/if}
						<IconButton
							icon="ri:information-line"
							label="Info"
							pressed={insp}
							onclick={() => (insp = !insp)}
						/>
						<IconButton icon="ri:more-line" label="More" haspopup="menu" onclick={openMore} />
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
					</div>
				</header>

				{#if err}
					<p class="error-msg">{err}</p>
				{/if}

				<!-- Its results, with a column beside them: All pages for an applet
				     that writes pages, the inspector when Info is open. A failure
				     shows where its result is missing, with the fix that fits. -->
				<div class="body" class:with-side={made === 'pages' || insp}>
					<div class="results">
						{#if made === 'pages'}
							{#if selectedPage}
								<article class="page">
									<h2 class="page-title">{selectedPage.title}</h2>
									<p class="page-by">
										Written by {action.name}, {when(selectedPage.written_at)} ·
										<TextAction inline onclick={() => openPage(selectedPage.page_id)}>Open page</TextAction>
									</p>
									{#if pageContent === null}
										<p class="empty">Loading…</p>
									{:else}
										<div class="page-body"><Markdown content={pageContent} variant="article" /></div>
									{/if}
								</article>
							{:else}
								<p class="empty">No pages yet. Its pages show here after it writes one.</p>
							{/if}
						{:else}
							{#if problem}
								<ul class="result-list" role="list">{@render problemRow()}</ul>
							{/if}
							{#if made === 'dashboard' && action.has_face}
								<FaceFrame appletId={action.id} height="460px" />
							{:else if made === 'dashboard'}
								<p class="empty">No dashboard yet. It's set to make one and hasn't yet.</p>
							{:else if results.length > 0}
								<section>
									<h2 class="list-head">{made === 'chat' ? 'Results' : 'Recent runs'}</h2>
									<ul class="result-list" role="list">
										{#each results as e (e.run_id ?? e.last_at)}
											<li class="result">
												<span class="result-text">
													{#if e.last_at}<span class="result-when">{longDay(e.last_at)}</span>{/if}
													<span class:failed={e.status === 'error'}>{made === 'chat' ? e.summary : runLine(e)}</span>
												</span>
											</li>
										{/each}
									</ul>
								</section>
							{:else if !problem}
								<p class="empty">
									{action.enabled ? 'Nothing yet. Its results show here after it runs.' : "It's off, so it hasn't run yet."}
								</p>
							{/if}
						{/if}
					</div>

					{#if insp}
						{@render inspector()}
					{:else if made === 'pages'}
						<nav class="page-list" aria-label={`Pages ${action.name} wrote`}>
							<h2 class="list-head">All pages</h2>
							{#if problem}
								<div class="page-item missing" title={problemText ?? undefined}>
									<span class="page-item-title">{missedWhen ? longDay(missedWhen) : 'Its last run'}</span>
									<span class="page-item-when">
										{failedNow ? "Wasn't written" : "Hasn't run"}{#if fix || canRunNow}{' · '}{@render fixAction()}{/if}
									</span>
								</div>
							{/if}
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
					{/if}
				</div>

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
		gap: 4px;
		flex: none;
	}
	.head-actions .switch {
		margin-left: 8px;
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


	/* Results, and beside them All pages or the inspector. */
	.body.with-side {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 264px;
		gap: 32px;
		align-items: start;
	}
	@container (max-width: 720px) {
		.body.with-side {
			grid-template-columns: 1fr;
		}
	}
	.results {
		display: flex;
		flex-direction: column;
		gap: 24px;
		min-width: 0;
	}

	.insp {
		display: flex;
		flex-direction: column;
		gap: 24px;
		padding-left: 24px;
		border-left: 1px solid var(--color-border);
	}
	.insp-text {
		margin: 0;
		font-size: 14px;
		line-height: 1.5;
	}
	.insp-text.muted {
		color: var(--color-foreground-muted);
	}
	.insp-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		width: 100%;
		min-height: 40px;
		padding: 8px 16px;
		border: 0;
		background: none;
		font: inherit;
		font-size: 14px;
		text-align: left;
		color: var(--color-foreground);
	}
	.insp-row + .insp-row {
		border-top: 1px solid var(--color-border);
	}
	.insp-row.link {
		cursor: pointer;
	}
	.insp-row.link:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}
	.insp-value {
		color: var(--color-foreground-muted);
		text-align: right;
	}

	/* A list of results, one line each, failures where they happened. */
	.result-list {
		list-style: none;
		margin: 0;
		padding: 0;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
	}
	.result {
		display: flex;
		align-items: center;
		gap: 16px;
		min-height: 48px;
		padding: 12px 16px;
	}
	.result + .result {
		border-top: 1px solid var(--color-border);
	}
	.result-text {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 14px;
		line-height: 1.5;
	}
	.result-when {
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.failed {
		color: var(--color-error);
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
	/* A row, so sans; the page's own title above stays serif. */
	.page-item-title {
		font-size: 14px;
	}
	.page-item.missing {
		cursor: default;
	}
	.page-item.missing .page-item-title,
	.page-item.missing .page-item-when {
		color: var(--color-error);
	}
	.page-item-when {
		font-size: 12px;
		color: var(--color-foreground-muted);
	}


	/* What its last runs did, one line each. */
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
