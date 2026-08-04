<!--
	One source: what it is, what's connected to it, and how to add another.

	This is where connections live. They were briefly an expandable row inside
	the catalog table, which was the only use of that grid feature anywhere in
	the app — a bespoke interaction invented for one screen. A row that opens a
	page is what every other list here does.

	Both kinds of connection appear: OAuth and API-key sources keep a credential
	row, device sources (iOS, Mac) pair into `app_device` and never mint one. The
	store makes them the same shape so this page doesn't have to care, and that
	is what finally files a paired iPhone under iOS instead of only under
	Settings → Devices.
-->
<script lang="ts">
	import { Page } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import { sourcesStore, type Connection } from '$lib/stores/sources.svelte';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { relativeTime } from '$lib/applets/palette';
	import { isMacOS, isTauri, thisComputerLabel } from '$lib/utils/platform';
	import { openExternal } from '$lib/tauri/bridge';

	let { sourceId }: { sourceId: string } = $props();

	const store = sourcesStore;

	$effect(() => {
		void store.load();
	});

	const source = $derived(store.catalogById.get(sourceId) ?? null);
	const connections = $derived(store.bySource.get(sourceId) ?? []);

	const isThisDevice = $derived(isTauri && isMacOS && sourceId === 'mac');

	const connectLabel = $derived.by(() => {
		if (!source) return '';
		if (isThisDevice) return `Set up ${thisComputerLabel}`;
		if (source.auth_kind === 'self_issued_bearer') return 'Pair a device';
		return connections.length > 0 ? 'Connect another' : 'Connect';
	});

	async function connect() {
		if (!source) return;
		if (isThisDevice) {
			windowShellStore.navigate('/virtues/this-mac', { label: 'This Mac' });
			return;
		}
		await connectFlow.start(source);
	}

	function openConnection(c: Connection) {
		if (c.route) windowShellStore.navigate(c.route, { label: c.name });
	}

	function readCode() {
		if (!source?.repo) return;
		void openExternal(
			source.repo_ref ? `${source.repo}/tree/main/${source.repo_ref}` : source.repo
		);
	}
</script>

<Page
	title={source?.name ?? sourceId}
	description={source?.description ??
		'This source is not installed on this box. Anything still connected to it will not run.'}
	maxWidth="wide"
>
	{#snippet actions()}
		{#if source}
			<button type="button" class="primary" onclick={() => void connect()}>{connectLabel}</button>
		{/if}
	{/snippet}

	{#if connectFlow.error}
		<div class="error">{connectFlow.error}</div>
	{/if}

	{#if source?.repo}
		<p class="repo">
			<Icon icon="ri:code-line" width="14" />
			<button type="button" class="link" onclick={readCode}>Read the code</button>
			{#if source.repo_ref}<code>{source.repo_ref}</code>{/if}
			<span class="aside">— provenance, not how it updates</span>
		</p>
	{/if}

	<h2>Connections</h2>
	{#if store.loading}
		<p class="muted">Loading…</p>
	{:else if connections.length === 0}
		<p class="muted">Nothing connected yet.</p>
	{:else}
		<ul class="connections">
			{#each connections as c (c.id)}
				<li class="connection" class:broken={c.broken}>
					<span class="dot" aria-hidden="true"></span>
					<button
						type="button"
						class="cname"
						class:inert={!c.route}
						disabled={!c.route}
						onclick={() => openConnection(c)}
					>
						{c.name}{c.isCurrent ? ' · this device' : ''}
					</button>
					<span class="cstatus">{c.statusLabel}</span>
					<span class="cseen">
						{c.lastSeenAt ? relativeTime(c.lastSeenAt) : 'no activity yet'}
					</span>
					<span class="capplets">
						{c.appletCount === null
							? ''
							: `${c.appletCount} ${c.appletCount === 1 ? 'applet' : 'applets'}`}
					</span>
				</li>
				{#if c.statusReason}
					<li class="reason">
						<Icon icon="ri:error-warning-line" width="14" />
						<span>{c.statusReason}</span>
					</li>
				{/if}
			{/each}
		</ul>
	{/if}
</Page>

<style>
	h2 {
		margin: 1.25rem 0 0.5rem;
		font-size: 0.9375rem;
		font-weight: 600;
		color: var(--color-foreground, #111827);
	}
	.muted {
		margin: 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.error {
		padding: 0.5rem 0.75rem;
		border-radius: 6px;
		background: var(--color-error-subtle);
		color: color-mix(in srgb, var(--color-error) 75%, #000);
		font-size: 0.8125rem;
	}

	.primary {
		padding: 0.375rem 0.75rem;
		border-radius: 6px;
		border: 1px solid var(--color-border, #d1d5db);
		background: var(--color-background, #fff);
		color: var(--color-foreground, #111827);
		font-size: 0.8125rem;
		font-weight: 500;
		cursor: pointer;
	}
	.primary:hover {
		background: var(--color-muted, #f3f4f6);
	}

	.repo {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		margin: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.repo code {
		font-size: 0.6875rem;
	}
	.aside {
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.link {
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
	.link:hover {
		text-decoration: underline;
	}

	.connections {
		list-style: none;
		margin: 0;
		padding: 0;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 10px;
		overflow: hidden;
	}
	.connection {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.5rem 0.875rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.connection + .connection {
		border-top: 1px solid var(--color-border, #e5e7eb);
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-success, #16a34a);
		flex-shrink: 0;
	}
	.connection.broken .dot {
		background: var(--color-error);
	}
	.cname {
		flex: 1;
		min-width: 0;
		text-align: left;
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--color-foreground, #111827);
		cursor: pointer;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.cname:hover:not(.inert) {
		text-decoration: underline;
	}
	.cname.inert {
		cursor: default;
	}
	.cstatus,
	.cseen {
		flex-shrink: 0;
		width: 9rem;
	}
	.connection.broken .cstatus {
		color: var(--color-error);
		font-weight: 500;
	}
	.capplets {
		flex-shrink: 0;
		width: 6rem;
		text-align: right;
	}
	.reason {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0 0.875rem 0.5rem 2rem;
		font-size: 0.6875rem;
		color: color-mix(in srgb, var(--color-error) 75%, #000);
	}

	@media (max-width: 720px) {
		.cstatus,
		.cseen,
		.capplets {
			display: none;
		}
	}
</style>
