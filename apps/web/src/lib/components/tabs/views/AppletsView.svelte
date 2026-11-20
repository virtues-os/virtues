<script lang="ts">
	/**
	 * The Sites door: everything you set up, as cards that each show a preview
	 * of the site, or as a list. The ones that need you come first.
	 *
	 * A card or row opens the site: its view when it has one, and otherwise
	 * the page that says how it runs. A site with a conversation carries a
	 * chat button in the list, which opens it beside the door.
	 *
	 * Built-in applets, with their days and runs, are in Settings
	 * (`settings/AppletsPage.svelte`).
	 */
	import type { Tab } from '$lib/tabs/types';
	import { Button, Page } from '$lib';
	import IconButton from '$lib/components/IconButton.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import ListRow from '$lib/components/datagrid/ListRow.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import NewAppletSheet from '$lib/components/applets/NewAppletSheet.svelte';
	import SiteCard from '$lib/components/applets/SiteCard.svelte';
	import { homeRoute } from '$lib/applets/routes';
	import { runApplet, type Applet } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { pinMenuItem } from '$lib/pins/pinAction';
	import { appletGlyph, describeTriggers } from '$lib/applets/palette';
	import { actionOf, canRun, problemOf, type AppletAction } from '$lib/applets/standing';
	import { needsYou } from '$lib/applets/days';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

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

	function chatOf(a: Applet): string | null {
		const id = a.config?.chat_id;
		return typeof id === 'string' && id ? id : null;
	}

	/** The row's one date: when it last ran, or that it is off. */
	function when(a: Applet): string {
		if (!a.enabled) return 'Off';
		const last = a.last_run?.started_at;
		return last ? formatRelativeTimestamp(last) : 'Not run yet';
	}


	// The columns are what search reads. Cards and rows draw themselves.
	const columns: Column<Applet>[] = [
		{ key: 'name', label: 'Name' },
		{ key: 'description', label: 'What it does', hidden: true },
	];

	function openHome(a: Applet) {
		windowShellStore.openTabFromRoute(homeRoute(a.id), { label: a.name, focusExisting: true });
	}

	function openConversation(a: Applet) {
		const chat = chatOf(a);
		if (!chat) return;
		windowShellStore.openAside({ type: 'chat', label: a.name, route: `/chat/${chat}`, icon: 'atlas:applets' });
	}

	function act(a: Applet, action: AppletAction) {
		if (action.kind === 'fix') windowShellStore.openTabFromRoute(action.href, { focusExisting: true });
		else void retry(a);
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
			{ id: 'home', label: 'Open site', icon: 'atlas:applets', action: () => openHome(a) },
		];
		if (chatOf(a)) {
			items.push({ id: 'chat', label: 'Open conversation', icon: 'mynaui:message', action: () => openConversation(a) });
		}
		if (canRun(a)) {
			items.push({ id: 'run', label: 'Run now', icon: 'mynaui:play', action: () => void retry(a) });
		}
		items.push(pinMenuItem({ url: homeRoute(a.id), label: a.name, icon: 'atlas:applets' }));
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}
</script>

<Page title="Sites" maxWidth="full">
	{#snippet actions()}
		<Button variant="primary" icon="mynaui:plus" onclick={() => (newSheetOpen = true)}>New site</Button>
	{/snippet}

	{#if err}
		<p class="error-msg">{err}</p>
	{/if}

	<UniversalDataGrid
		items={applets}
		{columns}
		entityType="sites"
		viewModes={['grid', 'list']}
		defaultViewMode="grid"
		gridMinWidth="264px"
		showCounts={false}
		pageSize={100000}
		loading={!appletsStore.loaded && !appletsStore.error}
		error={appletsStore.error && applets.length === 0 ? "Your server couldn't send your sites." : null}
		emptyIcon="atlas:applets"
		emptyMessage={'No sites yet. Ask in chat for something to look at or keep track of, and it appears here. A site is private until you share it.'}
		loadingMessage="Loading sites…"
		searchPlaceholder="Search sites"
		onItemClick={openHome}
		onItemContextMenu={rowMenu}
		onRetry={() => void appletsStore.load()}
	>
		{#snippet card(a: Applet)}
			<SiteCard site={a} />
		{/snippet}

		{#snippet listRow(a: Applet)}
			{@const trouble = problemOf(a)}
			<ListRow
				title={a.name}
				line={trouble ?? a.description ?? describeTriggers(a)}
				problem={!!trouble}
				note={when(a)}
			>
				{#snippet lead()}<AtlasIcon name={appletGlyph(a)} size={16} bare />{/snippet}
			</ListRow>
		{/snippet}

		{#snippet rowActions(a: Applet)}
			<!-- The fix as a word: Reconnect when the sign-in lapsed, Retry when
			     another try can work, Run now when a run didn't start. -->
			{@const action = actionOf(a)}
			{#if action}
				<TextAction disabled={action.kind === 'run' && retrying === a.id} onclick={() => act(a, action)}>
					{action.kind === 'run' && retrying === a.id ? 'Starting…' : action.label}
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

	{#if applets.length > 0}
		<p class="foot">Ask in chat for something to look at or keep track of, and it appears here. A site is private until you share it.</p>
	{/if}
</Page>

<NewAppletSheet open={newSheetOpen} onClose={() => (newSheetOpen = false)} />

<style>
	.foot {
		margin: 24px 0 0;
		font-size: var(--text-small);
		color: var(--color-foreground-subtle);
	}
	.error-msg {
		margin: 0 0 1rem;
		font-size: 0.875rem;
		color: var(--color-error);
	}
</style>
