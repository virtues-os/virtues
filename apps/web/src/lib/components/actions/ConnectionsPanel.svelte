<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import Button from '$lib/components/Button.svelte';
	import DevicePairModal from '$lib/components/sources/DevicePairModal.svelte';
	import ApiKeyConnectModal from '$lib/components/sources/ApiKeyConnectModal.svelte';
	import ProviderCatalog from './ProviderCatalog.svelte';
	import {
		listCredentials,
		listActions,
		revokeCredential,
		renameCredential,
		oauthStart,
		type Credential,
		type Action,
		type SourceCatalogItem
	} from '$lib/api/client';
	import { relativeTime } from '$lib/actions/palette';

	let credentials = $state<Credential[]>([]);
	let actions = $state<Action[]>([]);
	let loading = $state(true);
	let err = $state<string | null>(null);
	let expanded = $state<Record<string, boolean>>({});
	let pairModalOpen = $state(false);
	let pairModalDeviceType = $state<'ios' | 'mac'>('ios');
	let pairModalDisplayName = $state('iPhone');
	let pairModalSourceId = $state('ios');
	let apikeyModalOpen = $state(false);
	let apikeyModalSource = $state<SourceCatalogItem | null>(null);
	let renaming = $state<string | null>(null);
	let renameValue = $state('');
	let catalogRef = $state<{ refresh: () => void } | null>(null);

	// Dispatch a Connect click from the source catalog. Branch on auth.kind:
	//   self_issued_bearer → QR pair modal (iOS, Mac, custom paired devices)
	//   via_proxy          → POST /api/connect/:id/start → server-side redirect
	//   api_key            → form modal posting to /api/connect/:id/complete
	async function handleConnect(source: SourceCatalogItem) {
		err = null;

		if (source.auth_kind === 'self_issued_bearer') {
			pairModalSourceId = source.id;
			pairModalDeviceType = source.id === 'mac' ? 'mac' : 'ios';
			pairModalDisplayName = source.name;
			pairModalOpen = true;
			return;
		}

		if (source.auth_kind === 'via_proxy') {
			try {
				const { redirect_url } = await oauthStart(source.id, {
					return_url: `${window.location.origin}/oauth/callback`
				});
				window.location.assign(redirect_url);
			} catch (e) {
				err = e instanceof Error ? e.message : String(e);
			}
			return;
		}

		if (source.auth_kind === 'api_key') {
			apikeyModalSource = source;
			apikeyModalOpen = true;
			return;
		}

		err = `Unknown auth_kind for "${source.name}": ${source.auth_kind}`;
	}

	async function load() {
		loading = true;
		err = null;
		try {
			const [cs, as] = await Promise.all([listCredentials(), listActions()]);
			credentials = cs;
			actions = as;
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
	});

	// Group credentials by provider for rendering.
	const grouped = $derived.by(() => {
		const map = new Map<string, Credential[]>();
		for (const c of credentials) {
			const arr = map.get(c.provider) ?? [];
			arr.push(c);
			map.set(c.provider, arr);
		}
		return Array.from(map.entries());
	});

	function actionsForCredential(credentialId: string): Action[] {
		return actions.filter((a) => a.credential_id === credentialId);
	}

	function providerIcon(provider: string): string {
		switch (provider) {
			case 'ios':
				return 'ri:smartphone-line';
			default:
				return 'ri:cloud-line';
		}
	}

	function providerLabel(provider: string): string {
		switch (provider) {
			case 'ios':
				return 'iOS devices';
			default:
				return provider;
		}
	}

	async function handleRevoke(cred: Credential) {
		if (
			!confirm(
				`Revoke "${cred.name}"? This disables all ${cred.action_count} linked actions and requires re-pairing to restore.`
			)
		)
			return;
		try {
			await revokeCredential(cred.id);
			await load();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		}
	}

	function startRename(cred: Credential) {
		renaming = cred.id;
		renameValue = cred.name;
	}

	async function submitRename(cred: Credential) {
		const name = renameValue.trim();
		if (!name || name === cred.name) {
			renaming = null;
			return;
		}
		try {
			await renameCredential(cred.id, name);
			renaming = null;
			await load();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		}
	}

	function toggleExpanded(id: string) {
		expanded = { ...expanded, [id]: !expanded[id] };
	}
</script>

<section class="sources">
	<header class="section-header">
		<div>
			<h2>Sources</h2>
			<p class="subtitle">
				The providers that bring data into your system. Pick one to pair a device, sign in with OAuth, or paste an API key.
			</p>
		</div>
	</header>

	<ProviderCatalog bind:this={catalogRef} onConnect={handleConnect} />

	{#if err}
		<div class="error">{err}</div>
	{/if}

	{#if loading}
		<p class="muted">Loading…</p>
	{:else if credentials.length === 0}
		<div class="empty">
			<Icon icon="ri:link-unlink" width="32" />
			<p>No passwords yet — pick a source above to get started.</p>
		</div>
	{:else}
		<div class="connected-header">
			<h3>Passwords</h3>
		</div>
		{#each grouped as [provider, creds]}
			<div class="provider-group">
				<h3 class="provider-label">
					<Icon icon={providerIcon(provider)} width="16" />
					{providerLabel(provider)}
				</h3>
				<ul class="credential-list">
					{#each creds as cred}
						{@const linked = actionsForCredential(cred.id)}
						<li class="cred-card" class:inactive={!cred.is_active}>
							<div class="cred-top">
								<div class="cred-info">
									<div class="cred-icon">
										<Icon icon={providerIcon(provider)} width="20" />
									</div>
									<div>
										{#if renaming === cred.id}
											<input
												type="text"
												class="rename-input"
												bind:value={renameValue}
												onblur={() => submitRename(cred)}
												onkeydown={(e) => {
													if (e.key === 'Enter') submitRename(cred);
													else if (e.key === 'Escape') (renaming = null);
												}}
											/>
										{:else}
											<button
												type="button"
												class="cred-name"
												onclick={() => {
													if (cred.is_active) startRename(cred);
												}}
											>
												{cred.name}
											</button>
										{/if}
										<div class="cred-meta">
											{#if cred.is_active}
												<Badge variant="success">active</Badge>
											{:else}
												<Badge variant="warning">revoked</Badge>
											{/if}
											{#if cred.device_info}
												<span class="dim">{cred.device_info.device_model}</span>
												<span class="sep">·</span>
											{/if}
											<span class="dim">
												{cred.last_seen_at
													? `last seen ${relativeTime(cred.last_seen_at)}`
													: `paired ${relativeTime(cred.created_at)}`}
											</span>
										</div>
									</div>
								</div>
								<div class="cred-actions">
									<button
										type="button"
										class="chevron"
										aria-label="Toggle streams"
										onclick={() => toggleExpanded(cred.id)}
									>
										<Icon
											icon={expanded[cred.id] ? 'ri:arrow-up-s-line' : 'ri:arrow-down-s-line'}
											width="18"
										/>
									</button>
									{#if cred.is_active}
										<button
											type="button"
											class="kebab"
											aria-label="Revoke credential"
											title="Revoke credential"
											onclick={() => handleRevoke(cred)}
										>
											<Icon icon="ri:delete-bin-line" width="16" />
										</button>
									{/if}
								</div>
							</div>

							<div class="stream-summary">
								<Icon icon="ri:radar-line" width="12" />
								<span>
									{linked.length === 0
										? 'No streams yet'
										: `Ingesting ${linked.length} data stream${linked.length === 1 ? '' : 's'}`}
								</span>
							</div>

							{#if expanded[cred.id] && linked.length > 0}
								<ul class="stream-list">
									{#each linked as a}
										<li class="stream-item">
											<span class="stream-name">{a.name}</span>
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
												<span class="dim">{relativeTime(a.last_run.started_at)}</span>
											{:else}
												<span class="dim">no runs yet</span>
											{/if}
										</li>
									{/each}
								</ul>
							{/if}
						</li>
					{/each}
				</ul>
			</div>
		{/each}
	{/if}
</section>

<DevicePairModal
	deviceType={pairModalDeviceType}
	displayName={pairModalDisplayName}
	open={pairModalOpen}
	onClose={() => {
		pairModalOpen = false;
		void load();
		catalogRef?.refresh();
	}}
	onSuccess={() => {
		pairModalOpen = false;
		void load();
		catalogRef?.refresh();
	}}
/>

<ApiKeyConnectModal
	source={apikeyModalSource}
	open={apikeyModalOpen}
	onClose={() => {
		apikeyModalOpen = false;
	}}
	onSuccess={() => {
		apikeyModalOpen = false;
		void load();
		catalogRef?.refresh();
	}}
/>

<style>
	.sources {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}
	.section-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
	}
	.section-header h2 {
		margin: 0;
		font-size: 1.25rem;
		font-weight: 600;
	}
	.subtitle {
		margin: 0.125rem 0 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.connected-header h3 {
		margin: 0.25rem 0 0;
		font-size: 0.8125rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.provider-group {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.provider-label {
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-foreground-subtle, #9ca3af);
		display: flex;
		align-items: center;
		gap: 0.375rem;
		margin: 0;
	}

	.credential-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.cred-card {
		padding: 0.875rem 1rem;
		border-radius: 10px;
		border: 1px solid var(--color-border, #e5e7eb);
		background: var(--color-surface, #fff);
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.cred-card.inactive {
		opacity: 0.55;
	}

	.cred-top {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
	}
	.cred-info {
		display: flex;
		gap: 0.75rem;
		align-items: flex-start;
	}
	.cred-icon {
		display: grid;
		place-items: center;
		width: 36px;
		height: 36px;
		border-radius: 999px;
		background: var(--color-surface-elevated, #f3f4f6);
		color: var(--color-foreground-muted, #6b7280);
	}
	.cred-name {
		font-size: 0.9375rem;
		font-weight: 600;
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		color: inherit;
		text-align: left;
	}
	.cred-name:hover {
		text-decoration: underline dotted;
	}
	.rename-input {
		font: inherit;
		font-size: 0.9375rem;
		font-weight: 600;
		padding: 0.125rem 0.375rem;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 4px;
		background: var(--color-surface, #fff);
	}

	.cred-meta {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		margin-top: 0.25rem;
		font-size: 0.75rem;
	}
	.dim {
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.sep {
		opacity: 0.4;
	}

	.cred-actions {
		display: flex;
		gap: 0.25rem;
	}
	.chevron,
	.kebab {
		background: none;
		border: 1px solid transparent;
		border-radius: 6px;
		width: 28px;
		height: 28px;
		display: grid;
		place-items: center;
		cursor: pointer;
		color: var(--color-foreground-muted, #6b7280);
	}
	.chevron:hover,
	.kebab:hover {
		background: var(--color-surface-elevated, #f3f4f6);
		border-color: var(--color-border, #e5e7eb);
	}

	.stream-summary {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.stream-list {
		list-style: none;
		padding: 0.5rem 0.75rem;
		margin: 0;
		border-top: 1px solid var(--color-border-subtle, #f3f4f6);
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}
	.stream-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.75rem;
	}
	.stream-name {
		flex: 1;
	}

	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		padding: 3rem 1rem;
		color: var(--color-foreground-subtle, #9ca3af);
		text-align: center;
	}
	.empty p {
		margin: 0;
	}

	.muted {
		color: var(--color-foreground-subtle, #9ca3af);
		font-style: italic;
	}
	.error {
		padding: 0.5rem 0.75rem;
		border-radius: 6px;
		background: #fee2e2;
		color: #991b1b;
		font-size: 0.8125rem;
	}
</style>
