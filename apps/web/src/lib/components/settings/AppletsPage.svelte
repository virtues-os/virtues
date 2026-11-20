<script lang="ts">
	/**
	 * Settings · Jobs (/virtues/jobs): what your server runs on its own, and
	 * how its last runs went. People come here when something stopped, or to
	 * see exactly what their server does.
	 *
	 * It opens with one sentence (`appletsLead` in lib/applets/standing.ts)
	 * and the button that fixes what needs you. Then it lists jobs by what
	 * they are for: the ones that keep an account or a device current, one
	 * row per account, and the ones that build and keep your record. A job
	 * that needs you stays in its place. A row opens in place
	 * (`JobOpen.svelte`). What you set up yourself is in Sites.
	 */
	import { onMount } from 'svelte';
	import SettingsPage from './SettingsPage.svelte';
	import Fold from './Fold.svelte';
	import LoadError from './LoadError.svelte';
	import Button from '$lib/components/Button.svelte';
	import JobOpen from '$lib/components/applets/JobOpen.svelte';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { whenInWords } from '$lib/utils/dateUtils';
	import { adminReconcile, runApplet, type Applet } from '$lib/api/client';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { pinMenuItem } from '$lib/pins/pinAction';
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	import { openSettingsPage } from '$lib/settings/focus';
	import {
		actionOf,
		appletLine,
		appletsLead,
		canRun,
		groupJobs,
		livingJobs,
		problemOf,
		type AccountJobs,
		type AppletAction,
	} from '$lib/applets/standing';
	import { homeRoute, optionsRoute } from '$lib/applets/routes';

	let loaded = $state(false);
	let now = $state(Date.now());
	let running = $state<string | null>(null);
	let runError = $state<string | null>(null);
	let reconciling = $state(false);
	let reconcileNote = $state<string | null>(null);

	async function load() {
		// The accounts' names come with the sources. The list still reads without them.
		await Promise.all([appletsStore.load(), sourcesStore.load().catch(() => {})]);
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
	const groups = $derived(groupJobs(applets, now));
	const jobs = $derived(livingJobs(applets));
	const lead = $derived(appletsLead(jobs, now));
	/** The jobs that need you and have a button that fixes them, for under the lead. */
	const fixes = $derived(
		jobs.flatMap((a) => {
			const action = actionOf(a, now);
			return action ? [{ a, action }] : [];
		})
	);

	// One row is open at a time: a job, or an account with its jobs inside.
	let open = $state<string | null>(null);
	let openInside = $state<string | null>(null);
	function toggle(key: string) {
		open = open === key ? null : key;
		openInside = null;
	}

	/** An account's or a device's name: its source, and its own label when the source has several. */
	function accountName(g: AccountJobs): string {
		const c = sourcesStore.connections.find((x) => x.kind === g.kind && x.id === g.id);
		if (!c) return g.jobs[0].name;
		const source = sourcesStore.sourceLabel(c.sourceId);
		const several = sourcesStore.connections.filter((x) => x.sourceId === c.sourceId).length > 1;
		return several && c.name ? `${source} - ${c.name}` : source || c.name;
	}

	/** An account's line: the job that needs you, or when one of its jobs last ran. */
	function accountLine(g: AccountJobs): { text: string; warn: boolean } {
		const needing = g.jobs.find((a) => problemOf(a, now));
		if (needing) return { text: `${needing.name}: ${problemOf(needing, now)}.`, warn: true };
		const last = g.jobs
			.map((a) => a.last_run?.started_at)
			.filter((t): t is string => !!t)
			.sort()
			.at(-1);
		return { text: last ? `Ran ${whenInWords(last, now)}.` : "Hasn't run yet.", warn: false };
	}

	function openApplet(a: Applet, panel?: 'history') {
		windowShellStore.openAside({
			type: 'applet',
			label: a.name,
			route: panel ? optionsRoute(a.id, panel) : homeRoute(a.id),
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
		items.push({ id: 'open', label: 'Open', icon: 'atlas:applets', action: () => openApplet(a) });
		items.push({ id: 'history', label: 'Run history', icon: 'mynaui:clock-circle', action: () => openApplet(a, 'history') });
		if (canRun(a)) items.push({ id: 'run', label: 'Run now', icon: 'mynaui:play', action: () => void run(a) });
		items.push(pinMenuItem({ url: homeRoute(a.id), label: a.name, icon: 'atlas:applets' }));
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	async function reconcile() {
		reconciling = true;
		reconcileNote = null;
		try {
			const out = await adminReconcile();
			reconcileNote = `Your server read ${out.upserted} folder${out.upserted === 1 ? '' : 's'}.`;
			await load();
		} catch (e) {
			reconcileNote = `Your server couldn’t read the folders: ${e instanceof Error ? e.message : String(e)}`;
		} finally {
			reconciling = false;
		}
	}
</script>

{#snippet chevron(turned: boolean)}
	<svg class="chevron" class:turned viewBox="0 0 24 24" aria-hidden="true" width="15" height="15">
		<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" />
	</svg>
{/snippet}

{#snippet jobRow(a: Applet, isOpen: boolean, onopen: () => void)}
	<div class="job" role="listitem">
		<button type="button" class="open" aria-expanded={isOpen} onclick={onopen} oncontextmenu={(e) => rowMenu(a, e)}>
			<span class="words">
				<span class="name" class:off={!a.enabled || a.archived_at}>{a.name}</span>
				{#if a.description && !isOpen}<span class="what">{a.description}</span>{/if}
			</span>
			<span class="line" class:warn={!!problemOf(a, now)}>{appletLine(a, now)}</span>
			{@render chevron(isOpen)}
		</button>
		{#if isOpen}
			<JobOpen job={a} onchanged={() => void load()} />
		{/if}
	</div>
{/snippet}

{#snippet list(items: Applet[])}
	<div role="list">
		{#each items as a (a.id)}{@render jobRow(a, open === a.id, () => toggle(a.id))}{/each}
	</div>
{/snippet}

<SettingsPage
	title="Jobs"
	lede="What runs on its own on your server, and how its last runs went."
	back={{ route: '/virtues/system', label: 'Your server' }}
>
	{#if failed}
		<LoadError message="Your server couldn’t send its jobs." onretry={load} />
	{:else if !loaded}
		<p class="settings-sub">Asking your server about its jobs…</p>
	{:else}
		<p class="settings-lead">{lead.sentence}</p>
		<p class="settings-sub">{lead.sub}</p>
		{#if fixes.length > 0 && fixes.length <= 2}
			<div class="fixes">
				{#each fixes as { a, action } (a.id)}
					<Button variant="primary" disabled={running === a.id} onclick={() => act(a, action)}>
						{running === a.id ? 'Starting…' : fixes.length > 1 ? `${action.label} - ${a.name}` : action.label}
					</Button>
				{/each}
			</div>
		{/if}
		{#if runError}<p class="run-error" role="alert">{runError}</p>{/if}

		{#if groups.yoursNeedingYou.length}
			<p class="settings-sub">
				{groups.yoursNeedingYou.map((a) => a.name).join(', ')}
				{groups.yoursNeedingYou.length === 1 ? 'needs' : 'need'} you too. What you set up is in
				<button
					type="button"
					class="settings-link"
					onclick={() => windowShellStore.openTabFromRoute('/sites', { label: 'Sites', focusExisting: true })}>Sites</button
				>.
			</p>
		{/if}

		{#if groups.accounts.length}
			<section class="group">
				<h2 class="settings-label">Keeps your accounts and devices current</h2>
				<div role="list">
					{#each groups.accounts as g (g.kind + g.id)}
						{@const key = `${g.kind}:${g.id}`}
						{#if g.jobs.length === 1}
							{@render jobRow({ ...g.jobs[0], name: accountName(g) }, open === key, () => toggle(key))}
						{:else}
							{@const line = accountLine(g)}
							<div class="job" role="listitem">
								<button type="button" class="open" aria-expanded={open === key} onclick={() => toggle(key)}>
									<span class="words">
										<span class="name">{accountName(g)}</span>
										<span class="what">{g.jobs.map((a) => a.name).join(', ')}</span>
									</span>
									<span class="line" class:warn={line.warn}>{line.text}</span>
									{@render chevron(open === key)}
								</button>
								{#if open === key}
									<div class="inside" role="list">
										{#each g.jobs as a (a.id)}
											{@render jobRow(a, openInside === a.id, () => (openInside = openInside === a.id ? null : a.id))}
										{/each}
									</div>
								{/if}
							</div>
						{/if}
					{/each}
				</div>
			</section>
		{/if}

		{#if groups.server.length}
			<section class="group">
				<h2 class="settings-label">Builds and keeps your record</h2>
				{@render list(groups.server)}
			</section>
		{/if}

		<div class="tail">
			{#if groups.whenAsked.length}
				<Fold label="Runs only when asked" sub="Nothing starts these on its own. You or the assistant start them from a chat.">
					{@render list(groups.whenAsked)}
				</Fold>
			{/if}
			{#if groups.finished.length}
				<Fold label="Finished" sub="One-time jobs and sites that already ran. They keep what they made.">
					{@render list(groups.finished)}
				</Fold>
			{/if}
			<Fold label="For troubleshooting" sub="Pick up job and site folders that changed outside the app.">
				<Button variant="secondary" disabled={reconciling} onclick={reconcile}>
					{reconciling ? 'Reading…' : 'Re-read folders'}
				</Button>
				{#if reconcileNote}<p class="fold-text">{reconcileNote}</p>{/if}
			</Fold>
		</div>

		<p class="settings-aside">
			Jobs and sites sign in with the
			<button type="button" class="settings-link" onclick={() => openSettingsPage('/virtues/data/keys', DEFAULT_ASSISTANT_NAME)}
				>keys</button
			> on Your data. What you set up yourself is in Sites.
		</p>
	{/if}
</SettingsPage>

<style>
	.run-error {
		margin: 12px 0 0;
		font-size: 13px;
		color: var(--color-error);
	}

	.fixes {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 16px;
	}

	.group {
		padding: 32px 0 8px;
	}

	.job + .job {
		border-top: 1px solid var(--color-border);
	}

	.open {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, auto) auto;
		align-items: center;
		gap: 16px;
		width: 100%;
		min-height: 56px;
		padding: 8px 0;
		border: none;
		background: none;
		font: inherit;
		text-align: left;
		color: inherit;
		cursor: pointer;
	}

	.words {
		min-width: 0;
	}

	.name {
		display: block;
		font-size: 15px;
		color: var(--color-foreground);
	}

	.name.off {
		color: var(--color-foreground-muted);
	}

	.what {
		display: block;
		margin-top: 4px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	/* A failed run's reason can be long. Two lines here; the whole of it is inside. */
	.line {
		display: -webkit-box;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		overflow: hidden;
		max-width: 260px;
		font-size: 13px;
		line-height: 1.45;
		text-align: right;
		color: var(--color-foreground-muted);
	}

	.line.warn {
		color: var(--color-warning);
	}

	/* An account's jobs sit inside it, set in from its name. */
	.inside {
		padding: 0 0 8px 16px;
		border-top: 1px solid var(--color-border);
	}

	.chevron {
		color: var(--color-foreground-muted);
		transition:
			color var(--motion-hover-out) var(--ease-premium),
			transform var(--motion-hover-out) var(--ease-premium);
	}

	.chevron.turned {
		transform: rotate(90deg);
	}

	.open:hover .chevron {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	@container (max-width: 520px) {
		.open {
			grid-template-columns: minmax(0, 1fr) auto;
		}
		.line {
			grid-column: 1;
			grid-row: 2;
			max-width: none;
			text-align: left;
		}
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
