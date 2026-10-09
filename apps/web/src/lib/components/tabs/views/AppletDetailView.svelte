<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
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
		runApplet,
		messageApplet,
		type Applet,
		type AppletLogEntry,
		type RunDay
	} from '$lib/api/client';
	import { describeSchedule, relativeTime } from '$lib/applets/palette';
	import { indexRunDays } from '$lib/applets/days';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { explainRunError } from '$lib/sources/run-errors';

	/**
	 * An applet's home: what it made, first. Info, Run history and Technical
	 * details open in place, named by `?panel=` in the route so the table's
	 * dots and Last run cell can link straight to Run history.
	 */
	let { tab }: { tab: Tab; active: boolean } = $props();

	type Panel = 'home' | 'info' | 'history' | 'details';
	const PANEL_TITLE: Record<Exclude<Panel, 'home'>, string> = {
		info: 'Info',
		history: 'Run history',
		details: 'Technical details'
	};
	// Where each panel's Back goes: one level up, not wherever you came from.
	const PARENT: Record<Exclude<Panel, 'home'>, Panel> = {
		info: 'home',
		history: 'info',
		details: 'info'
	};

	const url = $derived(new URL(tab.route, 'http://localhost'));
	const appletId = $derived(url.pathname.match(/^\/(?:applet|action)\/(applet_[^/]+)$/)?.[1] ?? null);
	const panel = $derived.by((): Panel => {
		const p = url.searchParams.get('panel');
		return p === 'info' || p === 'history' || p === 'details' ? p : 'home';
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
			const [l, d] = await Promise.allSettled([getAppletLog(id), getRunsByDay(35)]);
			log = l.status === 'fulfilled' ? l.value : [];
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
	const latest = $derived(log.find((e) => e.status === 'success' && (e.summary || e.message)) ?? null);
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
		if (!action.enabled) return 'Off';
		const runs = describeSchedule(action.schedule);
		return lastEntry?.last_at ? `${runs} · Last ran ${relativeTime(lastEntry.last_at)}` : runs;
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
					<div class="title-block">
						<h1 class="title">{action.name}</h1>
						<p class="status">{status}</p>
					</div>
					<div class="head-actions">
						{#if chatId}
							<Button variant="secondary" size="sm" icon="ri:chat-3-line" onclick={openConversation}>
								Conversation
							</Button>
						{/if}
						<IconButton icon="ri:information-line" label="Info" variant="secondary" onclick={() => go('info')} />
						<IconButton icon="ri:more-line" label="More" variant="secondary" haspopup="menu" onclick={openMore} />
					</div>
				</header>

				{#if err}
					<p class="error-msg">{err}</p>
				{/if}

				{#if failedNow}
					<!-- Said once, as one mark and one sentence; the fix is the
					     page's ordinary buttons. -->
					<div class="problem">
						<DayDot state="failed" size="md" />
						<div class="problem-text">
							<p>
								The last run failed {relativeTime(failedNow.last_at)}.
								{#if failure}{failure.title}. {failure.remedy}{/if}
							</p>
							<div class="problem-actions">
								{#if canRunNow}
									<Button variant="secondary" size="sm" onclick={runNow} disabled={busy}>Retry</Button>
								{/if}
								{#if chatId}
									<Button variant="secondary" size="sm" onclick={openConversation}>Ask why</Button>
								{/if}
								<TextAction onclick={() => go('history')}>See the error</TextAction>
							</div>
						</div>
					</div>
				{:else if overdue}
					<div class="problem">
						<DayDot state="missed" size="md" />
						<div class="problem-text">
							<p>It was due {relativeTime(action.next_due_at)} and hasn't run.</p>
							<div class="problem-actions">
								{#if canRunNow}
									<Button variant="secondary" size="sm" onclick={runNow} disabled={busy}>Run now</Button>
								{/if}
								<TextAction onclick={() => go('history')}>See run history</TextAction>
							</div>
						</div>
					</div>
				{/if}

				<!-- The face IS the home when there is one. -->
				{#if action.has_face}
					<FaceFrame appletId={action.id} height="460px" />
					<ShareSheet
						open={sharing}
						producer={{ kind: 'applet', id: action.id }}
						onClose={() => (sharing = false)}
					/>
				{:else if latest}
					<section class="latest">
						<h2 class="latest-head">Latest, {relativeTime(latest.last_at)}</h2>
						{#if latest.message}
							<p class="said">{latest.message}</p>
						{/if}
						{#if latest.summary}
							<p class="latest-text">{latest.summary}</p>
						{/if}
					</section>
				{:else if action.last_success_summary}
					<section class="latest">
						<h2 class="latest-head">Latest</h2>
						<p class="latest-text">{action.last_success_summary}</p>
					</section>
				{:else if !failedNow}
					<p class="empty">
						{action.enabled ? "Nothing yet. What it makes shows here after it runs." : "It's off, so it hasn't made anything yet."}
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
			{:else}
				<header class="head sub">
					<TextAction quiet onclick={() => go(PARENT[panel])}>
						Back to {PARENT[panel] === 'home' ? action.name : 'Info'}
					</TextAction>
					<h1 class="title">{PANEL_TITLE[panel]}</h1>
					<p class="status">{action.name}{panel === 'history' ? ` · ${describeSchedule(action.schedule)}` : ''}</p>
				</header>

				{#if panel === 'info'}
					<AppletInfo
						bind:action
						{index}
						{daysErr}
						onHistory={() => go('history')}
						onTechnical={() => go('details')}
						onDeleted={() => windowShellStore.closeTab(tab.id)}
					/>
				{:else if panel === 'history'}
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
	}
	.head.sub {
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
		align-items: flex-start;
		gap: 12px;
	}
	.problem :global(.day-dot) {
		margin-top: 4px;
	}
	.problem-text p {
		margin: 0;
		font-size: 15px;
		line-height: 1.5;
	}
	.problem-actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px 16px;
		margin-top: 8px;
	}

	.latest {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.latest-head {
		margin: 0;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	.latest-text {
		margin: 0;
		max-width: 40em;
		font-family: var(--font-serif);
		font-size: 18px;
		line-height: 1.5;
		white-space: pre-wrap;
	}
	.said {
		margin: 0;
		padding-left: 8px;
		border-left: 2px solid var(--color-border);
		font-size: 14px;
		color: var(--color-foreground-muted);
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
