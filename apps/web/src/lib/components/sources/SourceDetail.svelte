<!--
	One source (/sources/<id>): whether it is working, what your server reads
	from it, how its last runs went, the accounts or devices connected to it,
	and Disconnect (agents/plan/sources-redesign-plan.md, slice 3).

	A connection has no page of its own. A second account is a row under
	Accounts, and `/sources/cred_…` forwards here (SourcesView). Both kinds of
	connection appear: OAuth and API-key sources keep a credential row, and
	device sources (iPhone, Mac) pair into `app_device`. The store gives them
	one shape, so this page only branches where they really differ: a device
	opens its own page in Settings, and an account can be renamed and
	disconnected here.

	Disconnecting keeps what your server already has. Deleting a source's
	records goes through `api::deletions`, which is not offered here yet.
-->
<script lang="ts">
	import { Page } from '$lib';
	import Button from '$lib/components/Button.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import Input from '$lib/components/Input.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { sourcesStore, type Connection } from '$lib/stores/sources.svelte';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import {
		getLakeStreams,
		getStreamDays,
		listApplets,
		renameCredential,
		revokeCredential,
		type Applet,
		type StreamDays
	} from '$lib/api/client';
	import { relativeTime } from '$lib/applets/palette';
	import { explainRunError } from '$lib/sources/run-errors';
	import { sourceMark } from '$lib/sources/brands';
	import type { LakeStream } from '$lib/sources/streams';
	import { isMacOS, isTauri, thisComputerLabel } from '$lib/utils/platform';
	import { openExternal } from '$lib/tauri/bridge';

	let { sourceId, tabId }: { sourceId: string; tabId: string } = $props();

	const store = sourcesStore;

	let applets = $state<Applet[]>([]);
	let days = $state<StreamDays[]>([]);
	let lake = $state<LakeStream[]>([]);
	let error = $state<string | null>(null);
	let busy = $state<string | null>(null);
	let renaming = $state<string | null>(null);
	let renameValue = $state('');

	const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

	async function loadExtras() {
		// Each part fails alone: a server without one endpoint still shows the rest.
		const [a, d, l] = await Promise.allSettled([
			listApplets(),
			getStreamDays(84),
			getLakeStreams<LakeStream[]>()
		]);
		applets = a.status === 'fulfilled' ? a.value : [];
		days = d.status === 'fulfilled' ? d.value.streams : [];
		lake = l.status === 'fulfilled' && Array.isArray(l.value) ? l.value : [];
	}

	$effect(() => {
		void sourceId;
		void store.load();
		void loadExtras();
	});

	const source = $derived(store.catalogById.get(sourceId) ?? null);
	const name = $derived(source?.name ?? sourceId);
	const mark = $derived(sourceMark(sourceId, source?.icon ?? null));
	const connections = $derived(store.bySource.get(sourceId) ?? []);
	const accounts = $derived(connections.filter((c) => c.kind === 'credential'));
	const devices = $derived(connections.filter((c) => c.kind === 'device'));
	const isDeviceSource = $derived(source?.auth_kind === 'self_issued_bearer');

	// A tab opened from a link says "Source" until the catalog names it. Only
	// write a label that differs, so this effect never rewrites its own value.
	$effect(() => {
		if (!source) return;
		const tab = windowShellStore.tabs.find((t) => t.id === tabId);
		if (tab && tab.label !== source.name) windowShellStore.updateTab(tabId, { label: source.name });
	});

	/** Under the name: who is connected, or what the source is when nothing is. */
	const subline = $derived(
		connections.length > 0 ? connections.map((c) => c.name).join(', ') : (source?.description ?? '')
	);

	const lastSeen = $derived.by(() => {
		const stamps = connections.map((c) => c.lastSeenAt).filter((v): v is string => v !== null);
		return stamps.length ? stamps.reduce((a, b) => (a > b ? a : b)) : null;
	});

	// ── Status ────────────────────────────────────────────────────────────
	const broken = $derived(connections.find((c) => c.broken) ?? null);

	const status = $derived.by(() => {
		if (!source) {
			return {
				line: `${name} is no longer installed.`,
				why: 'Nothing runs for it now. Disconnect what is left below if you no longer need it.'
			};
		}
		if (connections.length === 0) {
			return { line: `${name} isn't connected.`, why: '' };
		}
		if (broken) {
			return {
				line: `${name} needs you.`,
				why:
					broken.statusReason ??
					(broken.kind === 'device' ? 'It needs a permission on that device.' : 'Its sign-in stopped working.')
			};
		}
		const quiet = connections.find((c) => c.status === 'unreported');
		if (quiet?.statusReason) return { line: `${name} is connected.`, why: quiet.statusReason };
		if (lastSeen) return { line: `${name} is up to date.`, why: `It last sent something ${relativeTime(lastSeen)}.` };
		return { line: `${name} is connected.`, why: 'Nothing has arrived from it yet.' };
	});

	// ── What your server reads ────────────────────────────────────────────
	const streamRows = $derived(
		days
			.filter((s) => s.provider === sourceId && s.days.some((n) => n > 0))
			.sort((a, b) => a.display_name.localeCompare(b.display_name))
	);

	const keptSince = $derived.by(() => {
		const starts = lake
			.filter((s) => s.source_id === sourceId && s.earliest_at)
			.map((s) => s.earliest_at as string)
			.sort();
		if (starts.length === 0) return null;
		return new Date(starts[0]).toLocaleDateString('en-US', { month: 'long', year: 'numeric' });
	});

	/** The arrivals of one stream over the window, as a line in a 120×24 box. */
	function sparkline(values: number[]): string {
		const max = Math.max(1, ...values);
		const step = values.length > 1 ? 120 / (values.length - 1) : 0;
		return values.map((v, i) => `${(i * step).toFixed(1)},${(22 - (v / max) * 20).toFixed(1)}`).join(' ');
	}

	function lastArrival(values: number[]): string {
		const i = values.findLastIndex((n) => n > 0);
		if (i < 0) return '';
		const ago = values.length - 1 - i;
		return ago === 0 ? 'Today' : ago === 1 ? 'Yesterday' : `${ago} days ago`;
	}

	// ── Recent runs ───────────────────────────────────────────────────────
	const runs = $derived.by(() => {
		const creds = new Set(accounts.map((c) => c.id));
		const devs = new Set(devices.map((c) => c.id));
		return applets
			.filter((a) => (a.credential_id && creds.has(a.credential_id)) || (a.device_id && devs.has(a.device_id)))
			.filter((a) => a.last_run?.started_at)
			.sort((a, b) => (b.last_run!.started_at! > a.last_run!.started_at! ? 1 : -1))
			.slice(0, 3);
	});

	function runLine(a: Applet): string {
		const r = a.last_run!;
		const when = relativeTime(r.started_at!);
		if (r.status === 'success') return `Ran ${when}`;
		if (r.status === 'error') return `Failed ${when}`;
		return `Started ${when}`;
	}

	function runWhy(a: Applet): string | null {
		const r = a.last_run!;
		if (r.status !== 'error') return null;
		const denied = devices.flatMap((c) => c.denied);
		const known = explainRunError(r.error, denied);
		return known ? `${known.title}. ${known.remedy}` : r.error;
	}

	// ── Connecting and disconnecting ──────────────────────────────────────
	const isThisDevice = $derived(isTauri && isMacOS && sourceId === 'mac');

	const connectLabel = $derived.by(() => {
		if (!source) return '';
		if (isThisDevice) return `Set up ${thisComputerLabel}`;
		if (source.id === 'mac') return 'How to set up';
		if (isDeviceSource) return connections.length > 0 ? 'Pair another' : 'Pair';
		if (source.id === 'chat_import') return connections.length > 0 ? 'Import again' : 'Import';
		return connections.length > 0 ? `Add another ${name} account` : `Connect ${name}`;
	});

	async function connect() {
		if (!source) return;
		if (isThisDevice) {
			windowShellStore.navigate('/virtues/devices/this', { label: thisComputerLabel });
			return;
		}
		await connectFlow.start(source);
	}

	function fix(c: Connection) {
		if (source) void connectFlow.start(source, c.id);
	}

	function openDevice(c: Connection) {
		const route = c.isCurrent ? '/virtues/devices/this' : `/virtues/devices/${c.id}`;
		windowShellStore.navigate(route, { label: c.name });
	}

	async function disconnect(c: Connection) {
		const ok = await confirmAction({
			title: `Disconnect ${c.name}?`,
			body: `Your server stops reading it, and what uses it stops running. What your server already has from ${name} stays.`,
			confirmLabel: 'Disconnect',
			danger: true
		});
		if (!ok) return;
		busy = `out:${c.id}`;
		error = null;
		try {
			await revokeCredential(c.id);
			await store.load();
		} catch (e) {
			error = message(e);
		} finally {
			busy = null;
		}
	}

	function startRename(c: Connection) {
		renaming = c.id;
		renameValue = c.name;
	}

	async function submitRename(c: Connection) {
		const next = renameValue.trim();
		renaming = null;
		if (!next || next === c.name) return;
		try {
			await renameCredential(c.id, next);
			await store.load();
		} catch (e) {
			error = message(e);
		}
	}

	function readCode() {
		if (!source?.repo) return;
		void openExternal(source.repo_ref ? `${source.repo}/tree/main/${source.repo_ref}` : source.repo);
	}
</script>

<Page title={name} maxWidth="medium">
	<header class="head">
		<span class="tile" aria-hidden="true">
			<Icon icon={mark.icon} width="24" style={mark.color ? `color: ${mark.color}` : undefined} />
		</span>
		<div>
			<p class="title">{name}</p>
			{#if subline}<p class="sub">{subline}</p>{/if}
		</div>
	</header>

	<section class="status">
		<div>
			<p class="status-line">{status.line}</p>
			{#if status.why}<p class="status-why">{status.why}</p>{/if}
		</div>
		{#if broken && broken.kind === 'credential' && source}
			<Button variant="primary" onclick={() => fix(broken)}>Sign in to {name}</Button>
		{:else if source && connections.length === 0}
			<Button variant="primary" onclick={() => void connect()}>{connectLabel}</Button>
		{/if}
	</section>

	{#if error}<p class="error">{error}</p>{/if}
	{#if connectFlow.error}<p class="error">{connectFlow.error}</p>{/if}

	{#if connections.length > 0}
		<h2 class="group">What your server reads</h2>
		{#if keptSince}
			<p class="lead">Your server has kept what {name} sends since {keptSince}.</p>
		{/if}
		{#if streamRows.length === 0}
			<p class="lead">Nothing has arrived in the last twelve weeks.</p>
		{:else}
			<ul class="rows">
				{#each streamRows as s (s.name)}
					<li class="row streams">
						<span class="text">
							<span class="name">{s.display_name}</span>
							<span class="about">Last arrived {lastArrival(s.days).toLowerCase()}</span>
						</span>
						<svg class="spark" width="120" height="24" viewBox="0 0 120 24" aria-hidden="true">
							<polyline points={sparkline(s.days)} />
						</svg>
					</li>
				{/each}
			</ul>
		{/if}
	{:else if source?.provides?.length}
		<h2 class="group">What your server would read</h2>
		<p class="lead">{source.provides.join(', ')}.</p>
	{/if}

	{#if runs.length > 0}
		<div class="group-row">
			<h2 class="group">Recent runs</h2>
			<TextAction onclick={() => windowShellStore.navigate('/virtues/jobs', { label: 'Settings · Jobs' })}>All runs</TextAction>
		</div>
		<ul class="rows">
			{#each runs as a (a.id)}
				{@const why = runWhy(a)}
				<li class="row runs">
					<span class="text">
						<span class="name">{a.name}</span>
						{#if why}<span class="about wrap">{why}</span>{/if}
					</span>
					<span class="when" class:failed={a.last_run?.status === 'error'}>{runLine(a)}</span>
				</li>
			{/each}
		</ul>
	{/if}

	{#if connections.length > 0}
		<h2 class="group">{isDeviceSource ? 'Devices' : 'Accounts'}</h2>
		<ul class="rows">
			{#each devices as c (c.id)}
				<li>
					<button type="button" class="row link" onclick={() => openDevice(c)}>
						<span class="text">
							<span class="name">{c.name}{c.isCurrent ? ' (this device)' : ''}</span>
							<span class="about">{c.lastSeenAt ? `Last sent something ${relativeTime(c.lastSeenAt)}` : 'Nothing sent yet'}</span>
						</span>
						<Icon icon="mynaui:chevron-right" width="16" class="chevron" />
					</button>
				</li>
			{/each}
			{#each accounts as c (c.id)}
				<li class="row">
					<span class="text">
						{#if renaming === c.id}
							<Input
								bind:value={renameValue}
								label="Name"
								onblur={() => void submitRename(c)}
								onkeydown={(e: KeyboardEvent) => {
									if (e.key === 'Enter') void submitRename(c);
									else if (e.key === 'Escape') renaming = null;
								}}
							/>
						{:else}
							<span class="name">{c.name}</span>
							<span class="about" class:failed={c.broken}>
								{c.broken ? 'Sign-in expired' : c.lastSeenAt ? `Last sent something ${relativeTime(c.lastSeenAt)}` : 'Nothing sent yet'}
							</span>
						{/if}
					</span>
					<span class="actions">
						{#if c.broken && source}
							<TextAction onclick={() => fix(c)}>Sign in</TextAction>
						{/if}
						<TextAction onclick={() => startRename(c)}>Rename</TextAction>
						{#if accounts.length > 1 || !source}
							<TextAction loading={busy === `out:${c.id}`} disabled={busy !== null} onclick={() => void disconnect(c)}>
								Disconnect
							</TextAction>
						{/if}
					</span>
				</li>
			{/each}
		</ul>
		{#if source && !isThisDevice && source.id !== 'mac'}
			<p class="another"><TextAction onclick={() => void connect()}>{connectLabel}</TextAction></p>
		{/if}
	{/if}

	{#if source && accounts.length === 1}
		<section class="leave">
			<p>Disconnecting stops your server reading this account. What it already has from {name} stays.</p>
			<Button
				variant="danger"
				loading={busy === `out:${accounts[0].id}`}
				disabled={busy !== null}
				onclick={() => void disconnect(accounts[0])}
			>
				Disconnect {name}
			</Button>
		</section>
	{/if}

	{#if source?.repo}
		<p class="code"><TextAction onclick={readCode}>Read the code</TextAction> that collects from {name}.</p>
	{/if}
</Page>

<style>
	.head {
		display: flex;
		align-items: center;
		gap: 16px;
		margin: 4px 0 0;
	}
	.tile {
		width: 52px;
		height: 52px;
		box-sizing: border-box;
		display: flex;
		align-items: center;
		justify-content: center;
		flex: none;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-background);
		color: var(--color-foreground);
	}
	.title {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-title);
		line-height: 1.15;
		color: var(--color-foreground);
	}
	.sub {
		margin: 2px 0 0;
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
	}

	.status {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 24px;
		margin: 32px 0 0;
	}
	.status-line {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-heading);
		line-height: 1.25;
		color: var(--color-foreground);
	}
	.status-why {
		margin: 6px 0 0;
		max-width: 60ch;
		font-size: var(--text-label);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.error {
		margin: 16px 0 0;
		font-size: var(--text-label);
		color: var(--color-error);
	}

	.group {
		margin: 40px 0 4px;
		font-family: var(--font-serif);
		font-size: var(--text-heading);
		font-weight: 400;
		line-height: 1.3;
		color: var(--color-foreground);
	}
	.group-row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
	}
	.lead {
		margin: 0 0 4px;
		font-size: var(--text-label);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.rows > li {
		border-bottom: 1px solid var(--color-border-subtle);
	}
	.row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 20px;
		min-height: 60px;
		width: 100%;
		box-sizing: border-box;
	}
	.row.runs {
		min-height: 52px;
	}
	.link {
		padding: 0 8px;
		margin: 0 -8px;
		width: calc(100% + 16px);
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		color: inherit;
		text-align: left;
		cursor: pointer;
	}
	.link:hover {
		background: var(--hover-bg);
	}
	.link:active {
		background: var(--active-bg);
	}
	.row :global(.chevron) {
		color: var(--color-foreground-subtle);
	}

	.text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
		padding: 8px 0;
	}
	.name {
		font-size: var(--text-body);
		color: var(--color-foreground);
	}
	.about {
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
	}
	.about.wrap {
		white-space: normal;
	}
	.failed {
		color: var(--color-error);
	}
	.when {
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}
	.when.failed {
		color: var(--color-error);
	}
	.actions {
		display: flex;
		align-items: center;
		gap: 14px;
	}

	.spark polyline {
		fill: none;
		stroke: var(--color-primary);
		stroke-width: 1.5;
		stroke-linejoin: round;
		stroke-linecap: round;
	}

	.another {
		margin: 14px 0 0;
		font-size: var(--text-label);
	}

	.leave {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 24px;
		margin-top: 48px;
		padding-top: 24px;
		border-top: 1px solid var(--color-border-subtle);
	}
	.leave p {
		margin: 0;
		max-width: 46ch;
		font-size: var(--text-label);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.code {
		margin: 32px 0 0;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
	}
</style>
