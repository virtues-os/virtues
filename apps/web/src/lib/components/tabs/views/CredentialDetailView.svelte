<!--
	CredentialDetailView.svelte

	Detail page for a single credential (source connection). Mounts at
	`/sources/<credential_id>`. Shows:
	  - Source, name and status
	  - Device info (for self_issued_bearer pairings)
	  - Linked applets list with last-run badges
	  - Disconnect button
-->

<script lang="ts">
	import { type Tab, routeToEntityId } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import {
		listCredentials,
		listApplets,
		listSourceCatalog,
		revokeCredential,
		renameCredential,
		type Credential,
		type Applet,
		type SourceCatalogItem
	} from '$lib/api/client';
	import { relativeTime } from '$lib/applets/palette';

	let { tab }: { tab: Tab } = $props();

	const credentialId = $derived(routeToEntityId(tab.route) ?? '');

	let credential = $state<Credential | null>(null);
	let applets = $state<Applet[]>([]);
	let catalog = $state<SourceCatalogItem[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let renaming = $state(false);
	let renameValue = $state('');

	async function load() {
		if (!credentialId) {
			error = 'No credential id in route';
			loading = false;
			return;
		}
		loading = true;
		error = null;
		try {
			const [creds, acts, src] = await Promise.all([
				listCredentials(),
				listApplets(),
				listSourceCatalog()
			]);
			credential = creds.find((c) => c.id === credentialId) ?? null;
			applets = acts.filter((a) => a.credential_id === credentialId);
			catalog = src;
			if (!credential) error = 'Credential not found';
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
	});

	const sourceMeta = $derived.by(() =>
		credential ? catalog.find((s) => s.id === credential!.provider) : null
	);

	function sourceIcon(): string {
		if (sourceMeta?.icon) return sourceMeta.icon;
		if (credential?.provider === 'ios') return 'mynaui:mobile';
		if (credential?.provider === 'mac') return 'mynaui:desktop';
		return 'mynaui:cloud';
	}

	async function disconnect() {
		if (!credential) return;
		const ok = await confirmAction({
			title: `Disconnect ${credential.name}?`,
			body: 'Applets that use it stop running until you connect it again.',
			confirmLabel: 'Disconnect',
			danger: true,
		});
		if (!ok) return;
		try {
			await revokeCredential(credential.id);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}

	function startRename() {
		if (!credential) return;
		renaming = true;
		renameValue = credential.name;
	}

	async function submitRename() {
		if (!credential) return;
		const name = renameValue.trim();
		if (!name || name === credential.name) {
			renaming = false;
			return;
		}
		try {
			await renameCredential(credential.id, name);
			renaming = false;
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}

	function statusLabel(s: string): string {
		if (s === 'active') return 'Connected';
		if (s === 'revoked') return 'Disconnected';
		return s;
	}

	function backToSources() {
		windowShellStore.openTabFromRoute('/sources');
	}
</script>

<section class="cred-detail">
	{#if loading}
		<div class="state">
			<Icon icon="mynaui:spinner" width="20" />
			<span>Loading…</span>
		</div>
	{:else if error}
		<div class="state error">
			<Icon icon="mynaui:danger-circle" width="20" />
			<span>{error}</span>
		</div>
	{:else if credential}
		<header class="header">
			<button class="back" onclick={backToSources} type="button">
				<Icon icon="mynaui:arrow-left" width="14" />
				Sources
			</button>

			<div class="title-row">
				<div class="title-icon">
					<Icon icon={sourceIcon()} width="22" />
				</div>
				<div class="title-text">
					{#if renaming}
						<input
							type="text"
							class="rename-input"
							bind:value={renameValue}
							onblur={submitRename}
							onkeydown={(e) => {
								if (e.key === 'Enter') submitRename();
								else if (e.key === 'Escape') (renaming = false);
							}}
						/>
					{:else}
						<button class="title-name" type="button" onclick={startRename}>
							{credential.name}
						</button>
					{/if}
					<div class="title-meta">
						<span class="provider">{sourceMeta?.name ?? credential.provider}</span>
						<span class="sep">·</span>
						<span>{statusLabel(credential.status)}</span>
						{#if credential.last_seen_at}
							<span class="sep">·</span>
							<span class="dim">last seen {relativeTime(credential.last_seen_at)}</span>
						{/if}
					</div>
				</div>
				{#if credential.is_active}
					<Button variant="ghost" onclick={disconnect}>Disconnect</Button>
				{/if}
			</div>
		</header>

		{#if credential.device_info}
			<section class="card">
				<h2>Device</h2>
				<dl class="dl">
					<dt>Model</dt>
					<dd>{credential.device_info.device_model}</dd>
					<dt>Name</dt>
					<dd>{credential.device_info.device_name}</dd>
					<dt>OS version</dt>
					<dd>{credential.device_info.os_version}</dd>
					{#if credential.device_info.app_version}
						<dt>App version</dt>
						<dd>{credential.device_info.app_version}</dd>
					{/if}
					<dt>Device id</dt>
					<dd class="mono">{credential.device_info.device_id}</dd>
				</dl>
			</section>
		{/if}

		<section class="card">
			<h2>Applets</h2>
			{#if applets.length === 0}
				<p class="dim">No applets use it yet.</p>
			{:else}
				<ul class="applet-list">
					{#each applets as a}
						<li class="applet-row">
							<div class="applet-name">{a.name}</div>
							<div class="applet-meta">
								{#if a.last_run}
									<span class="dim small">
										{a.last_run.status === 'success' ? 'Ran' : a.last_run.status === 'error' ? 'Failed' : 'Started'}
										{relativeTime(a.last_run.started_at)}
									</span>
								{:else}
									<span class="dim small">Hasn't run yet</span>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}
</section>

<style>
	.cred-detail {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
		padding: 1rem 0.5rem;
		max-width: var(--page-detail);
	}

	.state {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.state.error {
		color: var(--color-error);
	}

	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		font-size: 0.75rem;
		padding: 0.25rem 0.5rem 0.25rem 0.375rem;
		margin-left: -0.375rem;
		border-radius: var(--radius-md);
		background: transparent;
		border: 1px solid transparent;
		color: var(--color-foreground-muted, #6b7280);
		cursor: pointer;
		align-self: flex-start;
	}
	.back:hover {
		background: var(--color-surface-elevated, #f3f4f6);
	}

	.header {
		display: flex;
		flex-direction: column;
		gap: 0.625rem;
	}
	.title-row {
		display: flex;
		align-items: center;
		gap: 0.875rem;
	}
	.title-icon {
		display: grid;
		place-items: center;
		width: 44px;
		height: 44px;
		border-radius: var(--radius-full);
		background: var(--color-surface-elevated, #f3f4f6);
		color: var(--color-foreground-muted, #6b7280);
		flex-shrink: 0;
	}
	.title-text {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.title-name {
		font-size: 1.25rem;
		font-weight: 600;
		text-align: left;
		background: none;
		border: none;
		padding: 0;
		color: inherit;
		cursor: pointer;
	}
	.title-name:hover {
		text-decoration: underline dotted;
	}
	.rename-input {
		font: inherit;
		font-size: 1.25rem;
		font-weight: 600;
		padding: 0.125rem 0.375rem;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: var(--radius-md);
		background: var(--color-surface, #fff);
	}
	.title-meta {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.8125rem;
	}
	.provider {
		color: var(--color-foreground-muted, #6b7280);
		font-weight: 500;
	}
	.sep {
		opacity: 0.4;
	}
	.dim {
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.small {
		font-size: 0.75rem;
	}

	.card {
		padding: 1rem 1.125rem;
		border-radius: var(--radius-xl);
		border: 1px solid var(--color-border, #e5e7eb);
		background: var(--color-surface, #fff);
	}
	.card h2 {
		margin: 0 0 0.625rem;
		font-size: 0.6875rem;
		font-weight: 600;
		color: var(--color-foreground-subtle, #9ca3af);
		display: flex;
		align-items: center;
		gap: 0.375rem;
	}

	.dl {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 0.375rem 1rem;
		margin: 0;
		font-size: 0.8125rem;
	}
	.dl dt {
		color: var(--color-foreground-subtle, #9ca3af);
		font-weight: 500;
	}
	.dl dd {
		margin: 0;
		color: var(--color-foreground, #111827);
	}
	.mono {
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		font-size: 0.75rem;
	}

	.applet-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.applet-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.5rem 0.625rem;
		border-radius: var(--radius-md);
		border: 1px solid var(--color-border-subtle, #f3f4f6);
		background: var(--color-surface-elevated, #f9fafb);
	}
	.applet-name {
		font-size: 0.8125rem;
		font-weight: 500;
	}
	.applet-meta {
		display: flex;
		align-items: center;
		gap: 0.375rem;
	}
</style>
