<script lang="ts">
	/**
	 * The Applets door: the applets you made, each one row, the ones that need
	 * you first.
	 *
	 * A row says what the applet is for, when it runs and when it last did, or
	 * what went wrong and the word that fixes it. Clicking it opens the
	 * applet's home. An applet with a conversation carries a chat button that
	 * opens it beside the list, so the list stays in view.
	 *
	 * The table of every applet, with its days and runs, is Settings > Applets
	 * (`ManageApplets.svelte`): checking applets is rarer than using them.
	 * Built-in applets are listed there, and here only when one needs you.
	 */
	import type { Tab } from '$lib/tabs/types';
	import { Button, Page } from '$lib';
	import IconButton from '$lib/components/IconButton.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import NewAppletSheet from '$lib/components/applets/NewAppletSheet.svelte';
	import { runApplet, type Applet } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { pinMenuItem } from '$lib/pins/pinAction';
	import { appletGlyph, describeTriggers, errorHeadline, relativeTime, runFix } from '$lib/applets/palette';
	import { needsYou } from '$lib/applets/days';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	const NEEDS_YOU = 'Needs you';

	let newSheetOpen = $state(false);
	let retrying = $state<string | null>(null);
	let err = $state<string | null>(null);

	$effect(() => {
		void appletsStore.load();
	});

	// Yours, the ones that need you first, then the ones that are on. A
	// built-in applet that needs you marks Settings instead.
	const applets = $derived(
		[...appletsStore.mine].sort(
			(a, b) =>
				Number(needsYou(b)) - Number(needsYou(a)) ||
				Number(!a.enabled) - Number(!b.enabled) ||
				a.name.localeCompare(b.name)
		)
	);
	const anyNeedsYou = $derived(applets.some((a) => needsYou(a)));

	function chatOf(a: Applet): string | null {
		const id = a.config?.chat_id;
		return typeof id === 'string' && id ? id : null;
	}

	// The same grace the scheduler uses before a run counts as late.
	const OVERDUE_GRACE_MS = 60 * 60 * 1000;

	/** What went wrong, in a sentence, or null when nothing did. */
	function problem(a: Applet): string | null {
		if (a.enabled && !a.archived_at && a.last_run?.status === 'error') {
			// The reason when the run gave one ("Gmail is disconnected"), which
			// says more than when it happened.
			const reason = a.last_run.error ? errorHeadline(a.last_run.error, 120) : null;
			return reason ?? `Last run failed ${relativeTime(a.last_run.started_at)}`;
		}
		if (a.enabled && a.next_due_at && Date.now() - new Date(a.next_due_at).getTime() > OVERDUE_GRACE_MS) {
			return `Was due ${relativeTime(a.next_due_at)} and hasn't run`;
		}
		return null;
	}

	/** The row's one date: when it last ran, or that it is off. */
	function when(a: Applet): string {
		if (!a.enabled) return 'Off';
		const last = a.last_run?.started_at;
		return last ? formatRelativeTimestamp(last) : 'Not run yet';
	}

	// "Run now" is refused unless the applet lists the manual trigger.
	const canRetry = (a: Applet) => a.enabled && !a.archived_at && a.triggers.includes('manual');
	const fixOf = (a: Applet) => (a.last_run?.status === 'error' ? runFix(a.last_run.error ?? null) : null);

	const columns: Column<Applet>[] = [
		{ key: 'name', label: 'Name' },
		{ key: 'description', label: 'What it does', hidden: true },
		{
			key: 'enabled',
			label: 'Status',
			hidden: true,
			groupable: true,
			groupOrder: [NEEDS_YOU],
			getValue: (a) => (needsYou(a) ? NEEDS_YOU : anyNeedsYou ? 'Everything else' : 'Your applets'),
		},
	];

	function openHome(a: Applet) {
		windowShellStore.openTabFromRoute(`/applet/${a.id}`, { label: a.name, focusExisting: true });
	}

	function openConversation(a: Applet) {
		const chat = chatOf(a);
		if (!chat) return;
		windowShellStore.openAside({ type: 'chat', label: a.name, route: `/chat/${chat}`, icon: 'atlas:applets' });
	}

	function openFix(a: Applet) {
		const fix = fixOf(a);
		if (fix) windowShellStore.openTabFromRoute(fix.href, { focusExisting: true });
	}

	function openManage() {
		windowShellStore.openTabFromRoute('/virtues/applets', { label: 'Settings' });
	}

	async function retry(a: Applet) {
		retrying = a.id;
		err = null;
		try {
			await runApplet(a.id);
			await appletsStore.load();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			retrying = null;
		}
	}

	function rowMenu(a: Applet, e: MouseEvent) {
		e.preventDefault();
		const items: ContextMenuItem[] = [
			{ id: 'home', label: 'Open applet', icon: 'atlas:applets', action: () => openHome(a) },
		];
		if (chatOf(a)) {
			items.push({ id: 'chat', label: 'Open conversation', icon: 'mynaui:message', action: () => openConversation(a) });
		}
		if (canRetry(a)) {
			items.push({ id: 'run', label: 'Run now', icon: 'mynaui:play', action: () => void retry(a) });
		}
		items.push(pinMenuItem({ url: `/applet/${a.id}`, label: a.name, icon: 'atlas:applets' }));
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}
</script>

<Page title="Applets" maxWidth="prose">
	{#snippet actions()}
		<Button variant="secondary" size="sm" onclick={openManage}>Manage applets</Button>
		<Button variant="primary" size="sm" icon="mynaui:plus" onclick={() => (newSheetOpen = true)}>New applet</Button>
	{/snippet}

	{#if err}
		<p class="error-msg">{err}</p>
	{/if}

	<UniversalDataGrid
		items={applets}
		{columns}
		entityType="applets-list"
		viewModes={['list']}
		defaultViewMode="list"
		defaultGroupBy="enabled"
		showCounts={false}
		pageSize={100000}
		loading={!appletsStore.loaded && !appletsStore.error}
		error={appletsStore.error && applets.length === 0 ? "Your server couldn't send your applets." : null}
		emptyIcon="atlas:applets"
		emptyMessage={'Nothing runs for you yet. Ask in chat - "write my examen each morning," "remind me on the 25th" - and it becomes an applet.'}
		loadingMessage="Loading applets…"
		searchPlaceholder="Search applets"
		onItemClick={openHome}
		onItemContextMenu={rowMenu}
		onRetry={() => void appletsStore.load()}
	>
		{#snippet listRow(a: Applet)}
			{@const trouble = problem(a)}
			<span class="row">
				<span class="row-icon" aria-hidden="true"><AtlasIcon name={appletGlyph(a)} size={16} bare /></span>
				<span class="row-text">
					<span class="row-title">{a.name}</span>
					<span class="row-line" class:problem={!!trouble}>{trouble ?? a.description ?? describeTriggers(a)}</span>
				</span>
				<span class="row-date">{when(a)}</span>
			</span>
		{/snippet}

		{#snippet rowActions(a: Applet)}
			<!-- The fix as a word: Reconnect when the sign-in lapsed, Retry when
			     another try can work. -->
			{#if problem(a) && fixOf(a)}
				<TextAction onclick={() => openFix(a)}>{fixOf(a)?.label}</TextAction>
			{:else if problem(a) && canRetry(a)}
				<TextAction disabled={retrying === a.id} onclick={() => retry(a)}>
					{retrying === a.id ? 'Retrying…' : 'Retry'}
				</TextAction>
			{/if}
			{#if chatOf(a)}
				<IconButton
					icon="mynaui:message"
					label={`Conversation with ${a.name}: ask it something or change it`}
					onclick={() => openConversation(a)}
				/>
			{/if}
		{/snippet}
	</UniversalDataGrid>
</Page>

<NewAppletSheet open={newSheetOpen} onClose={() => (newSheetOpen = false)} />

<style>
	.row {
		display: flex;
		align-items: flex-start;
		gap: 0.875rem;
	}
	.row-icon {
		flex: 0 0 16px;
		margin-top: 0.25rem;
		color: var(--color-foreground-muted);
	}
	.row-text {
		flex: 1 1 auto;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.row-title,
	.row-line {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.row-title {
		font-family: var(--font-serif);
		font-size: 1.125rem;
		line-height: 1.3;
		color: var(--color-foreground);
	}
	:global(.list-item:hover) .row-title {
		color: var(--color-foreground-muted);
	}
	.row-line {
		font-family: var(--font-serif);
		font-size: 1rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}
	.row-line.problem {
		color: var(--color-error);
	}
	.row-date {
		flex: 0 0 auto;
		margin-top: 0.25rem;
		font-size: 0.8125rem;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
	}
	.error-msg {
		margin: 0 0 1rem;
		font-size: 0.875rem;
		color: var(--color-error);
	}
</style>
