<script lang="ts">
	/**
	 * Settings → Connections: other services' MCP servers, whose tools the
	 * assistant can use when you ask in chat (agents/plan/mcp-plan.md part 1).
	 *
	 * The featured servers come first, then a search of the public MCP
	 * Registry, then any server address you paste. Connecting opens the
	 * service's own sign-in in your browser; the service sends the browser
	 * back to your server, which keeps the token. Nothing here syncs data.
	 */
	import { onMount } from 'svelte';
	import { Button, Input, Page, Section, TextAction } from '$lib';
	import { apiGet, apiSend } from '$lib/api/client';
	import { backendUrl } from '$lib/config/backend';
	import { openExternal } from '$lib/tauri/bridge';
	import { reloadOnReturn } from '$lib/components/sources/connectDispatch';

	type Connected = { id: string; name: string; about: string; url: string; registry: string | null };
	type Listing = { registry: string; title: string; about: string; url: string; is_connected: boolean };

	let connected = $state<Connected[]>([]);
	let featured = $state<Listing[]>([]);
	let registryError = $state<string | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let busy = $state<string | null>(null);

	let query = $state('');
	let results = $state<Listing[] | null>(null);
	let searching = $state(false);
	let address = $state('');

	async function load() {
		try {
			const list = await apiGet<{
				connected: Connected[];
				featured: Listing[];
				registry_error: string | null;
			}>('/connections');
			connected = list.connected;
			featured = list.featured;
			registryError = list.registry_error;
		} catch (e) {
			error = message(e);
		} finally {
			loading = false;
		}
	}

	onMount(load);

	const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

	// The search runs a moment after typing stops, so each keystroke is not a
	// request to the registry.
	let timer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		const q = query.trim();
		clearTimeout(timer);
		if (!q) {
			results = null;
			return;
		}
		timer = setTimeout(async () => {
			searching = true;
			try {
				const found = await apiGet<{ results: Listing[] }>('/connections/search', { q });
				if (query.trim() === q) results = found.results;
			} catch (e) {
				error = message(e);
			} finally {
				searching = false;
			}
		}, 300);
	});

	/** Starts a sign-in and opens it in the browser. */
	async function connect(target: { registry: string } | { url: string }, key: string) {
		busy = key;
		error = null;
		try {
			// The address your browser reaches the server by, where the
			// service sends it back after the sign-in.
			const origin = new URL(backendUrl('/'), location.href).origin;
			const { auth_url } = await apiSend<{ auth_url: string }>('POST', '/connections/connect', {
				origin,
				...target
			});
			reloadOnReturn(load);
			await openExternal(auth_url);
		} catch (e) {
			error = message(e);
		} finally {
			busy = null;
		}
	}

	async function disconnect(c: Connected) {
		busy = c.id;
		error = null;
		try {
			await apiSend('DELETE', `/connections/${encodeURIComponent(c.id)}`);
			await load();
		} catch (e) {
			error = message(e);
		} finally {
			busy = null;
		}
	}
</script>

{#snippet listing(l: Listing)}
	<li class="row">
		<div class="text">
			<span class="name">{l.title}</span>
			{#if l.about}<span class="about">{l.about}</span>{/if}
		</div>
		{#if l.is_connected}
			<span class="state">Connected</span>
		{:else}
			<Button
				variant="secondary"
				size="sm"
				loading={busy === l.registry}
				disabled={busy !== null}
				onclick={() => connect({ registry: l.registry }, l.registry)}
			>
				Connect
			</Button>
		{/if}
	</li>
{/snippet}

<Page
	title="Connections"
	description="Services the assistant can use when you ask in chat"
	maxWidth="wide"
>
	{#if error}<p class="error" role="alert">{error}</p>{/if}

	{#if connected.length > 0}
		<Section title="Connected" first>
			<ul class="rows">
				{#each connected as c (c.id)}
					<li class="row">
						<div class="text">
							<span class="name">{c.name}</span>
							<span class="about">
								In chat, ask for {c.id}.{c.about ? ` ${c.about}` : ''}
							</span>
						</div>
						<TextAction loading={busy === c.id} disabled={busy !== null} onclick={() => disconnect(c)}>
							Disconnect
						</TextAction>
					</li>
				{/each}
			</ul>
		</Section>
	{/if}

	<Section title="Featured" first={connected.length === 0}>
		{#if loading}
			<p class="quiet">Loading…</p>
		{:else if registryError}
			<p class="quiet">{registryError}</p>
		{:else}
			<ul class="rows">
				{#each featured as l (l.registry)}
					{@render listing(l)}
				{/each}
			</ul>
		{/if}
	</Section>

	<Section title="Find a server" note="MCP Registry">
		<p class="quiet">
			Anyone can publish a server to the registry. Connect only services you already trust. Servers
			that need an API key are not listed yet.
		</p>
		<Input type="text" bind:value={query} placeholder="Search by service" loading={searching} clearable />
		{#if results}
			{#if results.length === 0 && !searching}
				<p class="quiet">No server you can connect matches that.</p>
			{:else}
				<ul class="rows">
					{#each results as l (l.registry)}
						{@render listing(l)}
					{/each}
				</ul>
			{/if}
		{/if}
	</Section>

	<Section title="Add by address">
		<p class="quiet">For a server that is not in the registry. Its address starts with https://.</p>
		<div class="add">
			<Input type="url" bind:value={address} placeholder="https://mcp.example.com/mcp" />
			<Button
				variant="secondary"
				loading={busy === 'address'}
				disabled={busy !== null || !address.trim()}
				onclick={() => connect({ url: address.trim() }, 'address')}
			>
				Connect
			</Button>
		</div>
	</Section>
</Page>

<style>
	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.75rem 0;
		border-bottom: 1px solid var(--color-border-subtle);
	}
	.row:last-child {
		border-bottom: 0;
	}
	.text {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}
	.name {
		font-weight: 500;
		color: var(--color-foreground);
	}
	.about {
		font-size: 0.8125rem;
		line-height: 1.4;
		color: var(--color-foreground-muted);
		overflow: hidden;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
	}
	.state,
	.quiet {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.quiet {
		margin: 0 0 0.75rem;
		line-height: 1.5;
	}
	.state {
		flex-shrink: 0;
	}
	.add {
		display: flex;
		gap: 0.5rem;
		align-items: flex-start;
	}
	.add > :global(:first-child) {
		flex: 1;
	}
	.error {
		margin: 0 0 1rem;
		font-size: 0.875rem;
		color: var(--color-error, var(--color-foreground));
	}
</style>
