<!--
	CredentialDetailView.svelte

	Detail page for a single credential (source connection). Mounts at
	`/sources/<credential_id>`. Shows:
	  - Source + name + status badge
	  - Device info (for self_issued_bearer pairings)
	  - Linked actions list with last-run badges
	  - Revoke button
-->

<script lang="ts">
	import { type Tab, routeToEntityId } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import Button from '$lib/components/Button.svelte';
	import {
		listCredentials,
		listActions,
		listSourceCatalog,
		revokeCredential,
		renameCredential,
		type Credential,
		type Action,
		type SourceCatalogItem
	} from '$lib/api/client';
	import { relativeTime } from '$lib/actions/palette';

	let { tab }: { tab: Tab } = $props();

	const credentialId = $derived(routeToEntityId(tab.route) ?? '');

	let credential = $state<Credential | null>(null);
	let actions = $state<Action[]>([]);
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
				listActions(),
				listSourceCatalog()
			]);
			credential = creds.find((c) => c.id === credentialId) ?? null;
			actions = acts.filter((a) => a.credential_id === credentialId);
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
		if (credential?.provider === 'ios') return 'ri:smartphone-line';
		if (credential?.provider === 'mac') return 'ri:macbook-line';
		return 'ri:cloud-line';
	}

	async function handleRevoke() {
		if (!credential) return;
		const ok = await confirmAction({
			title: `Revoke "${credential.name}"?`,
			body: `This disables all ${credential.action_count} linked actions. You'll need to reconnect to restore them.`,
			confirmLabel: 'Revoke',
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

	function statusVariant(s: string): 'success' | 'warning' | 'muted' {
		if (s === 'active') return 'success';
		if (s === 'revoked') return 'warning';
		return 'muted';
	}

	function backToSources() {
		windowShellStore.openTabFromRoute('/sources');
	}
</script>

<section class="cred-detail">
	{#if loading}
		<div class="state">
			<Icon icon="ri:loader-4-line" width="20" />
			<span>Loading…</span>
		</div>
	{:else if error}
		<div class="state error">
			<Icon icon="ri:error-warning-line" width="20" />
			<span>{error}</span>
		</div>
	{:else if credential}
		<header class="header">
			<button class="back" onclick={backToSources} type="button">
				<Icon icon="ri:arrow-left-line" width="14" />
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
						<Badge variant={statusVariant(credential.status)}>{credential.status}</Badge>
						{#if credential.last_seen_at}
							<span class="sep">·</span>
							<span class="dim">last seen {relativeTime(credential.last_seen_at)}</span>
						{/if}
					</div>
				</div>
				{#if credential.is_active}
					<Button variant="ghost" onclick={handleRevoke}>
						<Icon icon="ri:delete-bin-line" width="14" />
						Revoke
					</Button>
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
			<h2>
				Actions
				<span class="count">{actions.length}</span>
			</h2>
			{#if actions.length === 0}
				<p class="dim">
					No actions linked to this credential yet.
					{#if credential.status === 'active'}
						The reconcile may still be fanning out — check back in a few seconds.
					{/if}
				</p>
			{:else}
				<ul class="action-list">
					{#each actions as a}
						<li class="action-row">
							<div class="action-name">{a.name}</div>
							<div class="action-meta">
								{#if a.last_run}
									<Badge
										variant={a.last_run.status === 'success'
											? 'success'
											: a.last_run.status === 'error'
												? 'error'
												: 'muted'}
									>
										{a.last_run.status}
									</Badge>
									<span class="dim small">{relativeTime(a.last_run.started_at)}</span>
								{:else}
									<span class="dim small">no runs yet</span>
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
		max-width: 64rem;
	}

	.state {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.state.error {
		color: color-mix(in srgb, var(--color-error) 75%, #000);
	}

	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		font-size: 0.75rem;
		padding: 0.25rem 0.5rem 0.25rem 0.375rem;
		margin-left: -0.375rem;
		border-radius: 6px;
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
		border-radius: 6px;
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
		border-radius: 10px;
		border: 1px solid var(--color-border, #e5e7eb);
		background: var(--color-surface, #fff);
	}
	.card h2 {
		margin: 0 0 0.625rem;
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-foreground-subtle, #9ca3af);
		display: flex;
		align-items: center;
		gap: 0.375rem;
	}
	.count {
		font-size: 0.6875rem;
		color: var(--color-foreground-muted, #6b7280);
		font-weight: 500;
		letter-spacing: normal;
		text-transform: none;
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

	.action-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.action-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.5rem 0.625rem;
		border-radius: 6px;
		border: 1px solid var(--color-border-subtle, #f3f4f6);
		background: var(--color-surface-elevated, #f9fafb);
	}
	.action-name {
		font-size: 0.8125rem;
		font-weight: 500;
	}
	.action-meta {
		display: flex;
		align-items: center;
		gap: 0.375rem;
	}
</style>
