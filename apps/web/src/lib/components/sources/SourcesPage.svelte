<!--
	Sources: one page for what your server reads from and the tools your
	assistant can use (agents/plan/sources-redesign-plan.md).

	Top to bottom:

	  - a sentence, only when something needs you, with the button that fixes
	    it when there is one thing to fix
	  - Connected: every source with a connection, and every connected tool
	  - Add a source: the catalog's sources with no connection yet
	  - Tools your assistant can use: featured MCP servers not yet connected,
	    and a link that opens a field to add any server by its address

	A source row opens that source's page. A tool has no page, so its row
	carries its own actions. Nothing here counts anything: no "3 connected",
	no totals, because someone in recovery reads this page too.
-->
<script lang="ts">
	import { Page } from '$lib';
	import Button from '$lib/components/Button.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import Input from '$lib/components/Input.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { sourcesStore, type Connection } from '$lib/stores/sources.svelte';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { apiGet, apiSend, type SourceCatalogItem } from '$lib/api/client';
	import { canSignInSheet, openSignIn } from '$lib/tauri/bridge';
	import { relativeTime } from '$lib/applets/palette';
	import { isMacOS, isTauri, thisComputerLabel } from '$lib/utils/platform';
	import { sourceMark, toolMark, type Mark } from '$lib/sources/brands';
	import { reloadOnReturn } from './connectDispatch';
	import type { Tab } from '$lib/tabs/types';

	let { tab }: { tab: Tab } = $props();

	const store = sourcesStore;

	/** A tool the assistant can act in, as `/api/connections` returns it.
	 *  `needs_sign_in` is absent from an older server. */
	type Connected = {
		id: string;
		name: string;
		about: string;
		url: string;
		registry: string | null;
		needs_sign_in?: boolean;
	};
	type Listing = { registry: string; title: string; about: string; url: string; is_connected: boolean };
	/** What happened to the service's own grant on disconnect. */
	type Grant = 'revoked' | 'not_offered' | 'not_revoked' | null;

	let tools = $state<Connected[]>([]);
	let featured = $state<Listing[]>([]);
	let toolsError = $state<string | null>(null);
	/** What the last disconnect did at the service, in a sentence. */
	let disconnected = $state<string | null>(null);
	/** The key of the connect or disconnect in flight, so its button spins. */
	let busy = $state<string | null>(null);
	let addingByAddress = $state(false);
	let address = $state('');

	const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

	async function loadTools() {
		try {
			const list = await apiGet<{ connected: Connected[]; featured: Listing[] }>('/connections');
			tools = list.connected;
			featured = list.featured;
			toolsError = null;
		} catch (e) {
			toolsError = message(e);
		}
	}

	$effect(() => {
		void store.load();
		void loadTools();
	});

	// ── After a sign-in ───────────────────────────────────────────────────
	// The OAuth callback returns as `?connected=<id>` or `?source=<id>&error=…`,
	// which the tab registry keeps on this tab's route. Read once at mount, then
	// point the tab at bare `/sources` so a refresh cannot replay a stale notice.
	const connectReturn = (() => {
		const q = tab.route.indexOf('?');
		if (q < 0) return null;
		const p = new URLSearchParams(tab.route.slice(q));
		const ok = p.get('connected');
		const error = p.get('error');
		const source = p.get('source');
		if (!ok && !error) return null;
		windowShellStore.updateTab(tab.id, { route: '/sources' });
		return { ok, error, source };
	})();
	let noticeDismissed = $state(false);

	const notice = $derived.by(() => {
		if (!connectReturn || noticeDismissed) return null;
		if (connectReturn.ok) return { good: true, text: `${store.sourceLabel(connectReturn.ok)} is connected.` };
		const who = connectReturn.source ? store.sourceLabel(connectReturn.source) : 'That source';
		if (connectReturn.error === 'connect_cancelled') {
			return { good: false, text: `${who} wasn't connected. The sign-in was closed before it finished.` };
		}
		if (connectReturn.error === 'bank_limit') {
			return { good: false, text: 'A subscription covers 10 linked banks. Remove one, then add this bank again.' };
		}
		return { good: false, text: `Couldn't finish connecting ${who}. Nothing was connected. Try again.` };
	});

	// Connections the server let go after the subscription lapsed. Said once:
	// dismissing it is remembered per viewer for that set of sources, and a
	// reconnect removes the source from the set.
	const RELEASED_KEY = 'sources.released-notice-dismissed';
	let releasedDismissed = $state(
		(() => {
			try {
				return typeof localStorage === 'undefined' ? '' : (localStorage.getItem(RELEASED_KEY) ?? '');
			} catch {
				return '';
			}
		})()
	);
	const releasedKey = $derived([...store.released].sort().join(','));
	const releasedNotice = $derived(
		store.released.length > 0 && releasedDismissed !== releasedKey
			? `Your ${joinNames(store.released.map((id) => store.sourceLabel(id)))} connections were removed when your subscription ended. Reconnect them anytime.`
			: null
	);
	function dismissReleased() {
		releasedDismissed = releasedKey;
		try {
			localStorage.setItem(RELEASED_KEY, releasedKey);
		} catch {
			// The notice comes back on the next visit; nothing else depends on it.
		}
	}

	// ── What needs you ────────────────────────────────────────────────────
	type Need = { name: string; reason: string; fix: (() => void) | null; fixLabel: string };

	const needs = $derived.by<Need[]>(() => {
		const out: Need[] = store.broken.map((c) => {
			const source = store.catalogById.get(c.sourceId) ?? null;
			const name = store.sourceLabel(c.sourceId);
			const isDevice = c.kind === 'device';
			return {
				name,
				reason:
					c.statusReason ??
					(isDevice ? 'It needs a permission on that device.' : 'Its sign-in stopped working.'),
				fix: !isDevice && source ? () => void connectFlow.start(source, c.id) : null,
				fixLabel: `Sign in to ${name}`
			};
		});
		for (const t of tools) {
			if (!t.needs_sign_in) continue;
			out.push({
				name: t.name,
				reason: 'Its sign-in expired, so your assistant cannot work there until you sign in again.',
				fix: () => void signInAgain(t),
				fixLabel: `Sign in to ${t.name}`
			});
		}
		return out;
	});

	const needsLine = $derived.by(() => {
		if (needs.length === 0) return null;
		const names = joinNames([...new Set(needs.map((n) => n.name))]);
		return needs.length === 1 ? `${names} needs you.` : `${names} need you.`;
	});

	function joinNames(names: string[]): string {
		if (names.length <= 1) return names[0] ?? '';
		return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
	}

	// ── Rows ──────────────────────────────────────────────────────────────
	type SourceRow = {
		source: SourceCatalogItem;
		mark: Mark;
		connections: Connection[];
		broken: boolean;
		lastSeen: string | null;
	};

	const sourceRows = $derived.by<SourceRow[]>(() =>
		store.catalog.map((source) => {
			const connections = store.bySource.get(source.id) ?? [];
			const stamps = connections.map((c) => c.lastSeenAt).filter((v): v is string => v !== null);
			return {
				source,
				mark: sourceMark(source.id, source.icon),
				connections,
				broken: connections.some((c) => c.broken),
				lastSeen: stamps.length ? stamps.reduce((a, b) => (a > b ? a : b)) : null
			};
		})
	);

	const connectedSources = $derived(sourceRows.filter((r) => r.connections.length > 0));
	// A box with nothing connected leads with the devices: what they send is
	// what other assistants don't have.
	const unconnectedSources = $derived.by(() => {
		const rest = sourceRows.filter((r) => r.connections.length === 0);
		if (connectedSources.length > 0) return rest;
		const isDevice = (r: SourceRow) => r.source.auth_kind === 'self_issued_bearer';
		return [...rest.filter(isDevice), ...rest.filter((r) => !isDevice(r))];
	});

	/** Connections whose source is no longer in the catalog. Nothing runs for
	 *  them, and this is the only place they can be found and removed. */
	const orphans = $derived.by(() => {
		const known = new Set(store.catalog.map((s) => s.id));
		return [...store.bySource.entries()].filter(([id]) => !known.has(id));
	});

	const unconnectedTools = $derived.by(() => {
		const have = new Set(tools.map((t) => t.registry));
		return featured.filter((l) => !have.has(l.registry));
	});

	function sourceLine(r: SourceRow): string {
		const about = r.source.description ?? '';
		if (!r.lastSeen) return about;
		return about ? `${about} · ${relativeTime(r.lastSeen)}` : relativeTime(r.lastSeen);
	}

	function brokenLabel(r: SourceRow): string {
		const c = r.connections.find((x) => x.broken);
		if (!c) return '';
		return c.kind === 'device' ? 'Needs a permission' : 'Sign-in expired';
	}

	// ── Connecting ────────────────────────────────────────────────────────
	/**
	 * Inside the Mac app the Mac is already here: it needs turning on, not
	 * pairing, and its device page owns that.
	 */
	function isThisDevice(source: SourceCatalogItem): boolean {
		return isTauri && isMacOS && source.id === 'mac';
	}

	function connectLabel(source: SourceCatalogItem): string {
		if (isThisDevice(source)) return `Set up ${thisComputerLabel}`;
		// A Mac is switched on inside that Mac's own app, never paired by a code.
		if (source.id === 'mac') return 'How to set up';
		if (source.auth_kind === 'self_issued_bearer') return 'Pair';
		if (source.id === 'chat_import') return 'Import';
		return 'Connect';
	}

	async function connect(source: SourceCatalogItem) {
		if (isThisDevice(source)) {
			windowShellStore.navigate('/virtues/devices/this', { label: thisComputerLabel });
			return;
		}
		await connectFlow.start(source);
	}

	function openSource(id: string, name: string) {
		windowShellStore.navigate(`/sources/${id}`, { label: name });
	}

	/**
	 * After a sign-in the service sends the browser to the OAuth proxy, and
	 * your server collects the code from there a few seconds later
	 * (`tools/connection.rs`). So coming back to the app reloads the list
	 * until the new connection shows, or a connection that needed a fresh
	 * sign-in no longer does, for up to half a minute.
	 */
	function reloadUntilConnected() {
		const shape = () => tools.map((c) => `${c.id}:${c.needs_sign_in === true}`).join();
		const before = shape();
		let tries = 0;
		const again = async () => {
			await loadTools();
			if (shape() === before && ++tries < 15) setTimeout(again, 2000);
		};
		reloadOnReturn(again);
	}

	/** Starts a tool's sign-in and opens it in the browser. */
	async function connectTool(target: { registry: string } | { url: string }, key: string) {
		busy = key;
		toolsError = null;
		disconnected = null;
		try {
			// `sheet` asks the proxy to close the iOS sign-in sheet once the
			// sign-in lands there.
			const sheet = await canSignInSheet();
			const { auth_url } = await apiSend<{ auth_url: string }>('POST', '/connections/connect', {
				...target,
				sheet
			});
			reloadUntilConnected();
			await openSignIn(auth_url);
		} catch (e) {
			toolsError = message(e);
		} finally {
			busy = null;
		}
	}

	/** Signs in again to the same server, which keeps the connection's name. */
	function signInAgain(t: Connected) {
		if (t.registry) return connectTool({ registry: t.registry }, t.id);
		return connectTool({ url: t.url }, t.id);
	}

	function grantNote(name: string, grant: Grant): string | null {
		switch (grant) {
			case 'revoked':
				return `Disconnected ${name}. ${name} confirmed your server's access has ended.`;
			case 'not_offered':
				return `Disconnected ${name}. ${name} gives your server no way to end its access, so remove Virtues in ${name}'s settings to end it there.`;
			case 'not_revoked':
				return `Disconnected ${name}, but ${name} did not confirm your server's access has ended. Remove Virtues in ${name}'s settings to be sure.`;
			default:
				return null;
		}
	}

	async function disconnectTool(t: Connected) {
		busy = `out:${t.id}`;
		toolsError = null;
		disconnected = null;
		try {
			// An older server answers 204 with no body.
			const answer = await apiSend<{ grant?: Grant } | undefined>(
				'DELETE',
				`/connections/${encodeURIComponent(t.id)}`
			);
			disconnected = grantNote(t.name, answer?.grant ?? null);
			await loadTools();
		} catch (e) {
			toolsError = message(e);
		} finally {
			busy = null;
		}
	}
</script>

{#snippet tile(mark: Mark)}
	<span class="tile" aria-hidden="true">
		<Icon icon={mark.icon} width="20" style={mark.color ? `color: ${mark.color}` : undefined} />
	</span>
{/snippet}

<Page title="Sources" description="What your server reads from, and the tools your assistant can use for you." maxWidth="medium">
	{#if notice}
		<p class="notice" class:good={notice.good}>
			<span>{notice.text}</span>
			<IconButton icon="mynaui:x" label="Dismiss" size="nested" onclick={() => (noticeDismissed = true)} />
		</p>
	{/if}
	{#if releasedNotice}
		<p class="notice">
			<span>{releasedNotice}</span>
			<IconButton icon="mynaui:x" label="Dismiss" size="nested" onclick={dismissReleased} />
		</p>
	{/if}

	{#if needsLine}
		<section class="needs" aria-label="Needs you">
			<div class="needs-text">
				<p class="needs-line">{needsLine}</p>
				{#if needs.length === 1}
					<p class="needs-why">{needs[0].reason}</p>
				{:else}
					<ul class="needs-list">
						{#each needs as n, i (i)}
							<li>
								<span>{n.name}: {n.reason}</span>
								{#if n.fix}
									<TextAction onclick={n.fix}>Sign in</TextAction>
								{/if}
							</li>
						{/each}
					</ul>
				{/if}
			</div>
			{#if needs.length === 1 && needs[0].fix}
				<Button variant="primary" onclick={needs[0].fix}>{needs[0].fixLabel}</Button>
			{/if}
		</section>
	{/if}

	{#if store.error}
		<p class="error">{store.error}</p>
	{/if}
	{#if connectFlow.error}
		<p class="error">{connectFlow.error}</p>
	{/if}

	{#if connectedSources.length > 0 || tools.length > 0 || orphans.length > 0}
		<h2 class="group">Connected</h2>
		<ul class="rows">
			{#each connectedSources as r (r.source.id)}
				<li>
					<button type="button" class="row link" onclick={() => openSource(r.source.id, r.source.name)}>
						{@render tile(r.mark)}
						<span class="text">
							<span class="name">{r.source.name}</span>
							<span class="about">{sourceLine(r)}</span>
						</span>
						{#if r.broken}
							<span class="state"><span class="dot" aria-hidden="true"></span>{brokenLabel(r)}</span>
						{:else}
							<Icon icon="mynaui:chevron-right" width="16" class="chevron" />
						{/if}
					</button>
				</li>
			{/each}
			{#each tools as t (t.id)}
				<li class="row">
					{@render tile(toolMark(t.registry))}
					<span class="text">
						<span class="name">{t.name}</span>
						<span class="about">{t.needs_sign_in ? 'Sign-in expired' : 'Your assistant can work here'}</span>
					</span>
					<span class="actions">
						{#if t.needs_sign_in}
							<Button variant="secondary" loading={busy === t.id} disabled={busy !== null} onclick={() => void signInAgain(t)}>
								Sign in
							</Button>
						{/if}
						<TextAction loading={busy === `out:${t.id}`} disabled={busy !== null} onclick={() => void disconnectTool(t)}>
							Disconnect
						</TextAction>
					</span>
				</li>
			{/each}
			{#each orphans as [id] (id)}
				<li>
					<button type="button" class="row link" onclick={() => openSource(id, id)}>
						{@render tile({ icon: 'mynaui:question-circle' })}
						<span class="text">
							<span class="name">{id}</span>
							<span class="about">No longer installed. Nothing runs for it. Open it to disconnect.</span>
						</span>
						<Icon icon="mynaui:chevron-right" width="16" class="chevron" />
					</button>
				</li>
			{/each}
		</ul>
		{#if disconnected}
			<p class="quiet">{disconnected}</p>
		{/if}
	{/if}

	{#if unconnectedSources.length > 0}
		<h2 class="group">Add a source</h2>
		<ul class="grid">
			{#each unconnectedSources as r (r.source.id)}
				<li class="row">
					{@render tile(r.mark)}
					<span class="text">
						<span class="name">{r.source.name}</span>
						<span class="about">{r.source.description ?? ''}</span>
					</span>
					<Button variant="secondary" onclick={() => void connect(r.source)}>{connectLabel(r.source)}</Button>
				</li>
			{/each}
		</ul>
	{/if}

	<h2 class="group">Tools your assistant can use</h2>
	<p class="lead">Your assistant works in these when you ask. They aren't synced.</p>
	{#if toolsError}
		<p class="error">{toolsError}</p>
	{/if}
	{#if unconnectedTools.length > 0}
		<ul class="grid">
			{#each unconnectedTools as l (l.registry)}
				<li class="row">
					{@render tile(toolMark(l.registry))}
					<span class="text">
						<span class="name">{l.title}</span>
						<span class="about">{toolMark(l.registry).about ?? l.about}</span>
					</span>
					<Button
						variant="secondary"
						loading={busy === l.registry}
						disabled={busy !== null}
						onclick={() => void connectTool({ registry: l.registry }, l.registry)}
					>
						Connect
					</Button>
				</li>
			{/each}
		</ul>
	{/if}

	{#if addingByAddress}
		<div class="address">
			<p class="quiet">
				Paste the address of the tool's MCP server. It starts with https://. Connect only tools you already
				trust.
			</p>
			<div class="address-field">
				<Input type="url" bind:value={address} placeholder="https://mcp.example.com/mcp" />
				<Button
					variant="secondary"
					loading={busy === 'address'}
					disabled={busy !== null || !address.trim()}
					onclick={() => void connectTool({ url: address.trim() }, 'address')}
				>
					Connect
				</Button>
			</div>
		</div>
	{:else}
		<p class="quiet another">
			Use another tool? <TextAction onclick={() => (addingByAddress = true)}>Add it by its address</TextAction>
		</p>
	{/if}
</Page>

<style>
	.notice {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		margin: 0 0 16px;
		padding: 8px 8px 8px 14px;
		border-radius: var(--radius-md);
		background: var(--fill-bg);
		font-size: var(--text-label);
		color: var(--color-foreground);
	}

	.needs {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 24px;
		margin: 8px 0 32px;
	}
	.needs-line {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-heading);
		line-height: 1.25;
		color: var(--color-foreground);
	}
	.needs-why {
		margin: 6px 0 0;
		max-width: 56ch;
		font-size: var(--text-label);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.needs-list {
		list-style: none;
		margin: 8px 0 0;
		padding: 0;
		font-size: var(--text-label);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.needs-list li {
		display: flex;
		gap: 10px;
		align-items: baseline;
	}

	.error {
		margin: 0 0 12px;
		font-size: var(--text-label);
		color: var(--color-error);
	}
	.quiet {
		margin: 12px 0 0;
		font-size: var(--text-label);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.group {
		margin: 36px 0 4px;
		font-family: var(--font-serif);
		font-size: var(--text-heading);
		font-weight: 400;
		line-height: 1.3;
		color: var(--color-foreground);
	}
	.lead {
		margin: 0 0 4px;
		font-size: var(--text-label);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.rows,
	.grid {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		column-gap: 40px;
	}
	@media (max-width: 640px) {
		.grid {
			grid-template-columns: minmax(0, 1fr);
		}
	}

	.row {
		display: grid;
		grid-template-columns: 40px minmax(0, 1fr) auto;
		align-items: center;
		gap: 14px;
		min-height: 64px;
		width: 100%;
		box-sizing: border-box;
	}
	.rows > li {
		border-bottom: 1px solid var(--color-border-subtle);
	}
	.grid .row {
		min-height: 72px;
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

	.tile {
		width: 40px;
		height: 40px;
		box-sizing: border-box;
		display: flex;
		align-items: center;
		justify-content: center;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		background: var(--color-background);
		color: var(--color-foreground);
	}

	.text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
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

	.state {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: var(--text-small);
		color: var(--color-error);
		white-space: nowrap;
	}
	.dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--color-error);
	}
	.row :global(.chevron) {
		color: var(--color-foreground-subtle);
	}
	.actions {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.another {
		margin-top: 16px;
	}
	.address {
		margin-top: 16px;
	}
	.address-field {
		display: flex;
		gap: 8px;
		margin-top: 8px;
	}
	.address-field > :global(:first-child) {
		flex: 1;
	}
</style>
