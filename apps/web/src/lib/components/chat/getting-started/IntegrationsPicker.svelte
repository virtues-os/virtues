<!--
	The integrations step, inline in the thread. A short table of the sources
	that pay off first, each with one line of what it holds and one button.
	Connecting happens in a modal over the room or, for OAuth, in the
	browser — never on a page beside it: the whole setup is this one
	conversation (Adam, 2026-09-14).

	The rows and their whys come from the same catalog and editorial layer
	the Sources room uses (sources-copy.ts), so the two cannot drift. Three
	rows lead; the rest of the catalog unfolds here, in place, on request.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import DevicePairModal from "$lib/components/sources/DevicePairModal.svelte";
	import ApiKeyConnectModal from "$lib/components/sources/ApiKeyConnectModal.svelte";
	import { connectIntent, reloadOnReturn } from "$lib/components/sources/connectDispatch";
	import { copyFor } from "$lib/components/onboarding/document/sources-copy";
	import { listSourceCatalog, listCredentials, type SourceCatalogItem, type Credential } from "$lib/api/client";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { GETTING_STARTED_CHAT_ID } from "./getting-started";
	import Act from "./ui/Act.svelte";

	/** The three that lead, in order. Everything else waits behind "Show all". */
	const LEAD = ["mac", "ios", "google"];

	let catalog = $state<SourceCatalogItem[]>([]);
	let credentials = $state<Credential[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let showAll = $state(false);
	let busy = $state<string | null>(null);

	// Modal state — one at a time, over the room.
	let pair = $state<{ deviceType: "ios" | "mac"; displayName: string } | null>(null);
	let apiKey = $state<SourceCatalogItem | null>(null);

	async function load() {
		try {
			const [src, creds] = await Promise.all([listSourceCatalog(), listCredentials()]);
			catalog = src;
			credentials = creds;
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}
	void load();

	/** Lead rows first, in LEAD order; then the rest by the catalog's own. */
	const rows = $derived.by(() => {
		const byId = new Map(catalog.map((s) => [s.id, s]));
		const lead = LEAD.map((id) => byId.get(id)).filter((s): s is SourceCatalogItem => !!s);
		if (!showAll) return lead;
		const rest = catalog.filter((s) => !LEAD.includes(s.id));
		return [...lead, ...rest];
	});

	function connected(s: SourceCatalogItem): boolean {
		return s.credential_count > 0;
	}
	function detailFor(s: SourceCatalogItem): string | null {
		const names = [...new Set(credentials.filter((c) => c.provider === s.id && c.is_active).map((c) => c.name))];
		if (names.length === 0) return null;
		return names.length > 1 ? `${names.length} accounts` : names[0];
	}

	async function connect(s: SourceCatalogItem) {
		if (busy) return;
		busy = s.id;
		error = null;
		try {
			const intent = await connectIntent(s, { next: `/chat/${GETTING_STARTED_CHAT_ID}` });
			if (intent.kind === "pair") pair = { deviceType: intent.deviceType, displayName: intent.displayName };
			else if (intent.kind === "api_key") apiKey = intent.source;
			else if (intent.kind === "oauth" && intent.external) reloadOnReturn(refresh);
			else if (intent.kind === "chat_import")
				error = "Chat history takes a file you export yourself, so it is done in Sources rather than here. Everything else on this list is one button.";
			else if (intent.kind === "error") error = intent.message;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			busy = null;
		}
	}

	async function refresh() {
		await Promise.all([load(), gettingStarted.refresh()]);
	}
</script>

<div class="picker">
	{#if loading}
		<p class="quiet-line">Looking at what your server can connect to…</p>
	{:else}
		<ul class="rows">
			{#each rows as s (s.id)}
				{@const copy = copyFor(s.id, s.description ?? "")}
				{@const on = connected(s)}
				<li class="row" class:on>
					<span class="mark" aria-hidden="true">
						{#if on}<Icon icon="ri:check-line" width="14" />{:else if s.icon}<Icon icon={s.icon} width="16" />{/if}
					</span>
					<span class="what">
						<span class="name">{s.name}</span>
						<span class="why">{on ? (detailFor(s) ?? "Connected") : copy.why}</span>
					</span>
					{#if on}
						<span class="state">Connected</span>
					{:else}
						<Act onclick={() => connect(s)} disabled={busy === s.id}>
							{busy === s.id ? "…" : "Connect"}
						</Act>
					{/if}
				</li>
			{/each}
		</ul>
		{#if !showAll}
			<Act variant="plain" onclick={() => (showAll = true)}>Show the rest</Act>
		{/if}
		{#if error}<p class="error">{error}</p>{/if}
	{/if}
</div>

<DevicePairModal
	deviceType={pair?.deviceType ?? "ios"}
	displayName={pair?.displayName ?? ""}
	open={pair !== null}
	onClose={() => (pair = null)}
	onSuccess={() => {
		pair = null;
		void refresh();
	}}
/>
<ApiKeyConnectModal
	source={apiKey}
	open={apiKey !== null}
	onClose={() => (apiKey = null)}
	onSuccess={() => {
		apiKey = null;
		void refresh();
	}}
/>

<style>
	.picker {
		margin: 0.625rem 0 0.25rem;
		max-width: 34rem;
	}
	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
		border-top: 1px solid var(--color-border);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.6rem 0;
		border-bottom: 1px solid var(--color-border);
	}
	.mark {
		width: 1.25rem;
		display: inline-flex;
		justify-content: center;
		color: var(--color-foreground-subtle);
		flex: none;
	}
	.row.on .mark {
		color: var(--color-foreground);
	}
	.what {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		min-width: 0;
		flex: 1;
	}
	.name {
		font-size: 1rem;
		color: var(--color-foreground);
	}
	.why {
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}
	.state {
		flex: none;
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}
	.quiet-line,
	.error {
		margin: 0.25rem 0 0;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}
	.error {
		color: var(--color-error, #9a2b2e);
	}
</style>
