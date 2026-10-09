<script lang="ts">
	/**
	 * The Applets page in Home: the applets you made, each one row, the ones
	 * that need you first.
	 *
	 * A row says when the applet runs and when it last did, or what went wrong
	 * and the button that fixes it. Clicking it opens the applet's home, which
	 * leads with what it made. Applets that have a conversation carry a chat
	 * button that opens it beside the list, so the list stays in view.
	 *
	 * The table of every applet, with its days and runs, is Settings > Applets
	 * (`ManageApplets.svelte`): checking applets is rarer than using them.
	 * Built-in applets are listed there, and here only when one needs you.
	 */
	import Button from '$lib/components/Button.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import { runApplet, type Applet } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { pinMenuItem } from '$lib/pins/pinAction';
	import { appletGlyph, describeSchedule, errorHeadline, relativeTime } from '$lib/applets/palette';
	import { needsYou } from '$lib/applets/days';
	import GitImportModal from './GitImportModal.svelte';
	import NewAppletSheet from './NewAppletSheet.svelte';

	let newSheetOpen = $state(false);
	let gitImportOpen = $state(false);
	let retrying = $state<string | null>(null);
	let err = $state<string | null>(null);

	$effect(() => {
		void appletsStore.load();
	});

	// Yours, plus any other applet only while it needs you (a backup with no
	// drive, a source whose sign-in lapsed).
	const shown = $derived([
		...appletsStore.mine,
		...appletsStore.list.filter(
			(a) => !appletsStore.mine.includes(a) && !a.archived_at && needsYou(a)
		)
	]);
	const problems = $derived(shown.filter((a) => needsYou(a)));
	const rest = $derived(
		shown
			.filter((a) => !needsYou(a))
			.sort((a, b) => Number(!a.enabled) - Number(!b.enabled) || a.name.localeCompare(b.name))
	);

	// Built-in applets keep the server running; a source's applets came with
	// something you connected. Folded at the bottom, so they're one click away
	// without the list opening on a dozen syncs. One that needs you is already
	// under Needs you above.
	const builtIn = $derived(
		appletsStore.list
			.filter((a) => (a.origin === 'system' || a.origin === 'source') && !a.archived_at && !needsYou(a))
			.sort((a, b) => a.name.localeCompare(b.name))
	);
	let builtInOpen = $state(false);
	const ORIGIN_TAG: Record<string, string> = { system: 'Built-in', source: 'Source' };

	function chatOf(a: Applet): string | null {
		const id = a.config?.chat_id;
		return typeof id === 'string' && id ? id : null;
	}

	// The same grace the scheduler uses before a run counts as late.
	const OVERDUE_GRACE_MS = 60 * 60 * 1000;

	function line(a: Applet): { text: string; problem: boolean } {
		if (a.enabled && !a.archived_at && a.last_run?.status === 'error') {
			// The reason when the run gave one ("Gmail is disconnected"), which
			// says more than when it happened.
			const reason = a.last_run.error ? errorHeadline(a.last_run.error, 120) : null;
			return { text: reason ?? `Last run failed ${relativeTime(a.last_run.started_at)}`, problem: true };
		}
		if (
			a.enabled &&
			a.next_due_at &&
			Date.now() - new Date(a.next_due_at).getTime() > OVERDUE_GRACE_MS
		) {
			return { text: `Was due ${relativeTime(a.next_due_at)} and hasn't run`, problem: true };
		}
		if (!a.enabled) return { text: 'Off', problem: false };
		const runs = describeSchedule(a.schedule);
		const last = a.last_run?.started_at;
		return { text: last ? `${runs} · last ran ${relativeTime(last)}` : runs, problem: false };
	}

	// "Run now" is refused unless the applet lists the manual trigger.
	const canRetry = (a: Applet) => a.enabled && !a.archived_at && a.triggers.includes('manual');

	function openHome(a: Applet) {
		windowShellStore.openTabFromRoute(`/applet/${a.id}`, { label: a.name, focusExisting: true });
	}

	function openConversation(a: Applet) {
		const chat = chatOf(a);
		if (!chat) return;
		windowShellStore.openAside({ type: 'chat', label: a.name, route: `/chat/${chat}`, icon: 'atlas:applets' });
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

	function tryAgain() {
		void appletsStore.load();
		void chatSessions.load();
	}

	function rowMenu(a: Applet, e: MouseEvent) {
		e.preventDefault();
		const items: ContextMenuItem[] = [
			{ id: 'home', label: 'Open applet', icon: 'atlas:applets', action: () => openHome(a) }
		];
		if (chatOf(a)) {
			items.push({ id: 'chat', label: 'Open conversation', icon: 'ri:chat-3-line', action: () => openConversation(a) });
		}
		if (canRetry(a)) {
			items.push({ id: 'run', label: 'Run now', icon: 'ri:play-line', action: () => void retry(a) });
		}
		items.push(pinMenuItem({ url: `/applet/${a.id}`, label: a.name, icon: 'atlas:applets' }));
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}
</script>

{#snippet row(a: Applet)}
	{@const l = line(a)}
	<li class="row">
		<button type="button" class="open" onclick={() => openHome(a)} oncontextmenu={(e) => rowMenu(a, e)}>
			<span class="glyph" aria-hidden="true"><AtlasIcon name={appletGlyph(a)} size={24} bare /></span>
			<span class="body">
				<span class="name-line">
					<span class="name">{a.name}</span>
					{#if ORIGIN_TAG[a.origin]}<span class="origin-tag">{ORIGIN_TAG[a.origin]}</span>{/if}
				</span>
				<span class="line" class:problem={l.problem}>{l.text}</span>
			</span>
		</button>
		{#if l.problem && canRetry(a)}
			<Button variant="secondary" size="sm" disabled={retrying === a.id} onclick={() => retry(a)}>
				{retrying === a.id ? 'Retrying…' : 'Retry'}
			</Button>
		{/if}
		{#if chatOf(a)}
			<IconButton
				icon="ri:chat-3-line"
				label={`Open the conversation with ${a.name}`}
				variant="secondary"
				class="chat-btn"
				onclick={() => openConversation(a)}
			/>
		{/if}
	</li>
{/snippet}

<section class="applets-panel">
	<header class="section-header">
		<div>
			<h2>Applets</h2>
			<p class="subtitle">Ask Virtues to do something for you, on a schedule or when something happens.</p>
		</div>
		<div class="header-actions">
			<Button variant="secondary" size="sm" onclick={openManage}>Manage applets</Button>
			<Button variant="primary" size="sm" icon="ri:add-line" onclick={() => (newSheetOpen = true)}>
				New applet
			</Button>
		</div>
	</header>

	{#if err}
		<p class="error-msg">{err}</p>
	{/if}

	{#if !appletsStore.loaded && !appletsStore.error}
		<p class="note">Loading…</p>
	{:else if appletsStore.error && shown.length === 0}
		<!-- What failed and one thing to do. If the server itself is down, the
		     app's "Can't reach your server" bar says so and offers the fix;
		     Try again re-runs the check that raises it. -->
		<div class="load-failed">
			<p class="error-msg" title={appletsStore.error}>Your server couldn't send your applets.</p>
			<Button variant="secondary" size="sm" onclick={tryAgain}>Try again</Button>
		</div>
	{:else if shown.length === 0}
		<p class="note">
			Nothing runs for you yet. Ask in chat - "write my examen each morning," "remind me on the 25th" - and it
			becomes an applet.
		</p>
	{:else}
		{#if problems.length > 0}
			<h3 class="group">Needs you</h3>
			<ul class="rows" role="list">
				{#each problems as a (a.id)}{@render row(a)}{/each}
			</ul>
		{/if}
		{#if rest.length > 0}
			<h3 class="group">{problems.length > 0 ? 'Everything else' : 'Your applets'}</h3>
			<ul class="rows" role="list">
				{#each rest as a (a.id)}{@render row(a)}{/each}
			</ul>
		{/if}
	{/if}

	{#if appletsStore.loaded && builtIn.length > 0}
		<button
			type="button"
			class="fold-head"
			aria-expanded={builtInOpen}
			onclick={() => (builtInOpen = !builtInOpen)}
		>
			<span>Built-in and sources ({builtIn.length})</span>
			<Icon icon={builtInOpen ? 'ri:arrow-down-s-line' : 'ri:arrow-right-s-line'} width="16" />
		</button>
		{#if builtInOpen}
			<ul class="rows" role="list">
				{#each builtIn as a (a.id)}{@render row(a)}{/each}
			</ul>
		{/if}
	{/if}
</section>

<NewAppletSheet
	open={newSheetOpen}
	onClose={() => (newSheetOpen = false)}
	onImport={() => {
		newSheetOpen = false;
		gitImportOpen = true;
	}}
/>

<GitImportModal open={gitImportOpen} onClose={() => (gitImportOpen = false)} onImported={() => appletsStore.load()} />

<style>
	.applets-panel {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.section-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
		flex-wrap: wrap;
		margin-bottom: 16px;
	}
	/* Matches PageHeading's level-1 title and its description, so a hand-rolled
	   header still reads as a page title (agents/build/design-grammar.md §4). */
	.section-header h2 {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 36px;
		line-height: 1.15;
		font-weight: 400;
	}
	.subtitle {
		margin: 8px 0 0;
		font-size: 15px;
		color: var(--color-foreground-muted);
	}
	.header-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.group {
		margin: 16px 0 0;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 400px), 1fr));
		gap: 4px 32px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 72px;
		padding: 0 8px;
		border-radius: 12px;
	}
	.row:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}
	.open {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 16px;
		padding: 12px 0;
		border: 0;
		background: none;
		font: inherit;
		color: inherit;
		text-align: left;
		cursor: pointer;
	}
	.glyph {
		width: 48px;
		height: 48px;
		flex: none;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 12px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground-muted);
	}
	.body {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}
	.name,
	.line {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.name {
		min-width: 0;
		font-size: 16px;
		color: var(--color-foreground);
	}
	.line {
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.line.problem {
		color: var(--color-error);
	}
	/* The conversation is its own action on the row, so it reads as a round
	   button beside the row rather than a glyph inside it (a pill, §6). */
	.row :global(.v-iconbtn.chat-btn) {
		width: 40px;
		height: 40px;
		border-radius: 999px;
		flex: none;
	}
	.fold-head {
		align-self: flex-start;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin-top: 24px;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.fold-head:hover {
		color: var(--color-foreground);
	}
	.name-line {
		display: flex;
		align-items: center;
		gap: 8px;
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
	.load-failed {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.note {
		margin: 0;
		font-size: 15px;
		color: var(--color-foreground-muted);
	}
	.error-msg {
		margin: 0;
		font-size: 14px;
		color: var(--color-error);
	}
</style>
