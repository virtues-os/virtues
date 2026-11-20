<script lang="ts">
	/**
	 * Settings · Applets (/virtues/applets): what runs on its own on your
	 * server, and how its last runs went. For checking and fixing; the Applets
	 * page in Home is where you use them.
	 *
	 * It opens with one sentence (`appletsLead` in lib/applets/standing.ts),
	 * then lists every applet in groups: what needs you, with the button that
	 * fixes it; yours; the ones that came with your server; and the ones that
	 * came with an account or device you connected. Each row says when the
	 * applet runs and how its last run went, in words. A row opens the
	 * applet's page, which has its run history.
	 */
	import { onMount } from 'svelte';
	import SettingsPage from './SettingsPage.svelte';
	import Fold from './Fold.svelte';
	import LoadError from './LoadError.svelte';
	import Button from '$lib/components/Button.svelte';
	import { adminReconcile, runApplet, type Applet } from '$lib/api/client';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { pinMenuItem } from '$lib/pins/pinAction';
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	import { openSettingsPage } from '$lib/settings/focus';
	import { actionOf, appletLine, appletsLead, canRun, groupApplets, type AppletAction } from '$lib/applets/standing';

	let loaded = $state(false);
	let now = $state(Date.now());
	let running = $state<string | null>(null);
	let runError = $state<string | null>(null);
	let reconciling = $state(false);
	let reconcileNote = $state<string | null>(null);

	async function load() {
		await appletsStore.load();
		loaded = true;
		now = Date.now();
	}

	onMount(() => {
		void load();
		// The rows say "4 minutes ago", so the clock they read moves on its own.
		const timer = setInterval(() => (now = Date.now()), 60_000);
		return () => clearInterval(timer);
	});

	const applets = $derived(appletsStore.list);
	const failed = $derived(loaded && !!appletsStore.error && applets.length === 0);
	const groups = $derived(groupApplets(applets, now));
	const lead = $derived(appletsLead(applets, now));
	const needing = $derived(new Set(groups.needsYou.map((a) => a.id)));
	const hasOwn = $derived(applets.some((a) => !a.archived_at && (a.origin === 'user' || a.origin === 'ai')));

	function openApplet(a: Applet, panel?: 'history') {
		windowShellStore.openAside({
			type: 'applet',
			label: a.name,
			route: panel ? `/applet/${a.id}?panel=${panel}` : `/applet/${a.id}`,
			icon: 'atlas:applets',
		});
	}

	function chatOf(a: Applet): string | null {
		const id = a.config?.chat_id;
		return typeof id === 'string' && id ? id : null;
	}

	async function run(a: Applet) {
		running = a.id;
		runError = null;
		try {
			await runApplet(a.id);
			await load();
		} catch (e) {
			runError = `${a.name} couldn’t start: ${e instanceof Error ? e.message : String(e)}`;
		} finally {
			running = null;
		}
	}

	function act(a: Applet, action: AppletAction) {
		if (action.kind === 'fix') void windowShellStore.openTabFromRoute(action.href, { focusExisting: true });
		else void run(a);
	}

	function rowMenu(a: Applet, e: MouseEvent) {
		e.preventDefault();
		const items: ContextMenuItem[] = [];
		const chat = chatOf(a);
		if (chat) {
			items.push({
				id: 'chat',
				label: 'Open conversation',
				icon: 'mynaui:message',
				action: () => void windowShellStore.openAside({ type: 'chat', label: a.name, route: `/chat/${chat}`, icon: 'atlas:applets' }),
			});
		}
		items.push({ id: 'open', label: 'Open applet', icon: 'atlas:applets', action: () => openApplet(a) });
		items.push({ id: 'history', label: 'Run history', icon: 'mynaui:clock-circle', action: () => openApplet(a, 'history') });
		if (canRun(a)) items.push({ id: 'run', label: 'Run now', icon: 'mynaui:play', action: () => void run(a) });
		items.push(pinMenuItem({ url: `/applet/${a.id}`, label: a.name, icon: 'atlas:applets' }));
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	async function reconcile() {
		reconciling = true;
		reconcileNote = null;
		try {
			const out = await adminReconcile();
			reconcileNote = `Your server read ${out.upserted} applet folder${out.upserted === 1 ? '' : 's'}.`;
			await load();
		} catch (e) {
			reconcileNote = `Your server couldn’t read the folders: ${e instanceof Error ? e.message : String(e)}`;
		} finally {
			reconciling = false;
		}
	}
</script>

{#snippet row(a: Applet)}
	{@const action = actionOf(a, now)}
	<div class="applet" role="listitem" oncontextmenu={(e) => rowMenu(a, e)}>
		<button type="button" class="open" onclick={() => openApplet(a)}>
			<span class="words">
				<span class="name" class:off={!a.enabled || a.archived_at}>{a.name}</span>
				<span class="line" class:warn={needing.has(a.id)}>{appletLine(a, now)}</span>
			</span>
			{#if !action}
				<svg class="chevron" viewBox="0 0 24 24" aria-hidden="true" width="15" height="15">
					<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" />
				</svg>
			{/if}
		</button>
		{#if action}
			<Button variant="secondary" disabled={running === a.id} onclick={() => act(a, action)}>
				{running === a.id ? 'Starting…' : action.label}
			</Button>
		{/if}
	</div>
{/snippet}

{#snippet list(applets: Applet[])}
	<div role="list">
		{#each applets as a (a.id)}{@render row(a)}{/each}
	</div>
{/snippet}

<SettingsPage
	title="Applets"
	lede="What runs on its own on your server, and how its last runs went."
	back={{ route: '/virtues/system', label: 'Your server' }}
>
	{#if failed}
		<LoadError message="Your server couldn’t send your applets." onretry={load} />
	{:else if !loaded}
		<p class="settings-sub">Asking your server about its applets…</p>
	{:else}
		<p class="settings-lead">{lead.sentence}</p>
		<p class="settings-sub">{lead.sub}</p>
		{#if runError}<p class="run-error" role="alert">{runError}</p>{/if}

		{#if groups.needsYou.length}
			<section class="group">
				<h2 class="settings-label">Needs you</h2>
				{@render list(groups.needsYou)}
			</section>
		{/if}

		{#if groups.yours.length || !hasOwn}
			<section class="group">
				<h2 class="settings-label">Yours</h2>
				{#if groups.yours.length}
					<p class="group-sub">
						You use these in
						<button
							type="button"
							class="settings-link"
							onclick={() => windowShellStore.openTabFromRoute('/applets', { label: 'Applets', focusExisting: true })}>Applets</button
						>. Here is how their runs went.
					</p>
					{@render list(groups.yours)}
				{:else}
					<p class="group-sub">
						Nothing you made runs yet. Ask in chat, such as “write my examen each morning” or “a dashboard of my heart rate”, and it
						becomes an applet.
					</p>
				{/if}
			</section>
		{/if}

		{#if groups.server.length}
			<section class="group">
				<h2 class="settings-label">Came with your server</h2>
				<p class="group-sub">These keep your server running.</p>
				{@render list(groups.server)}
			</section>
		{/if}

		{#if groups.connected.length}
			<section class="group">
				<h2 class="settings-label">From your accounts and devices</h2>
				<p class="group-sub">Each came with an account or a device you connected.</p>
				{@render list(groups.connected)}
			</section>
		{/if}

		<div class="tail">
			{#if groups.finished.length}
				<Fold label="Finished" sub="One-time applets that already ran. They keep what they made.">
					{@render list(groups.finished)}
				</Fold>
			{/if}
			<Fold label="For troubleshooting" sub="Pick up applet folders that changed outside the app.">
				<Button variant="secondary" disabled={reconciling} onclick={reconcile}>
					{reconciling ? 'Reading…' : 'Re-read applet folders'}
				</Button>
				{#if reconcileNote}<p class="fold-text">{reconcileNote}</p>{/if}
			</Fold>
		</div>

		<p class="settings-aside">
			Applets sign in with the
			<button type="button" class="settings-link" onclick={() => openSettingsPage('/virtues/data/keys', DEFAULT_ASSISTANT_NAME)}
				>keys</button
			> on Your data. Each applet’s page has its run history.
		</p>
	{/if}
</SettingsPage>

<style>
	.run-error {
		margin: 12px 0 0;
		font-size: 13px;
		color: var(--color-error);
	}

	.group {
		padding: 24px 0 8px;
	}

	.group-sub {
		margin: 0 0 4px;
		max-width: 60ch;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.applet {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 16px;
	}

	.open {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 16px;
		min-height: 60px;
		padding: 8px 0;
		border: none;
		background: none;
		font: inherit;
		text-align: left;
		color: inherit;
		cursor: pointer;
	}

	.name {
		display: block;
		font-size: 15px;
		color: var(--color-foreground);
	}

	.name.off {
		color: var(--color-foreground-muted);
	}

	.line {
		display: block;
		margin-top: 4px;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.line.warn {
		color: var(--color-warning);
	}

	.chevron {
		color: var(--color-foreground-muted);
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.open:hover .chevron {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	.tail {
		margin-top: 24px;
	}

	.fold-text {
		margin: 8px 0 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	@media (prefers-reduced-motion: reduce) {
		.chevron {
			transition: none;
		}
	}
</style>
