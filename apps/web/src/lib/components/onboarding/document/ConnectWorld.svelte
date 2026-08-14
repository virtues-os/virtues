<!--
  ConnectWorld — the "Your world" chapter body.

  A curated, editorial sources view (not the /sources admin grid). We surface
  only the few sources worth starting with — Google (the anchor), then the
  richest: iPhone, Mac, chat history. The long tail (finances, notes, fitness)
  waits in the app; onboarding is not a catalog. Privacy is stated once, not
  repeated on every row. Connect flow is the shared `connectIntent` dispatch.
  The device is platform-aware (live collector on a Mac; honest "coming" note
  on Windows/Linux).
-->
<script lang="ts">
	import Modal from "$lib/components/Modal.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import DevicePairModal from "$lib/components/sources/DevicePairModal.svelte";
	import ApiKeyConnectModal from "$lib/components/sources/ApiKeyConnectModal.svelte";
	import ChatImportCard from "$lib/components/onboarding/ChatImportCard.svelte";
	import CollectorPermissionCard from "$lib/components/onboarding/CollectorPermissionCard.svelte";
	import { connectIntent, reloadOnReturn } from "$lib/components/sources/connectDispatch";
	import { listSourceCatalog, listCredentials, type SourceCatalogItem } from "$lib/api/client";
	import { isTauri, isMacOS, isWindows, isLinux, thisComputerLabel } from "$lib/utils/platform";
	import { copyFor, PROMINENCE_ORDER, type Prominence } from "./sources-copy";
	import Marginalia from "./Marginalia.svelte";
	import SourceRow from "./SourceRow.svelte";

	interface Props {
		/** Called whenever a source connects, so the shell can refresh derived state. */
		onConnected: () => void;
		/** Called the moment the local Mac collector finishes (optimistic). */
		onDeviceReady?: () => void;
	}

	let { onConnected, onDeviceReady }: Props = $props();

	let catalog = $state<SourceCatalogItem[]>([]);
	let err = $state<string | null>(null);

	const localMac = $derived(isTauri && isMacOS);

	async function load() {
		try {
			const [src] = await Promise.all([listSourceCatalog(), listCredentials()]);
			catalog = src;
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		}
	}
	$effect(() => {
		void load();
	});

	// Only the anchor + the richest sources are shown in onboarding; the rest
	// (the "quiet" prominence) live in the app under Sources.
	const featured = $derived.by(() => {
		const want: Prominence[] = ["anchor", "prominent"];
		return catalog
			.filter((s) => !s.id.startsWith("__"))
			.map((s) => ({ source: s, copy: copyFor(s.id, s.description ?? "") }))
			.filter((x) => want.includes(x.copy.prominence))
			.sort((a, b) => {
				// Group first, then rank within it. Sorting on prominence alone
				// left ties to catalog order, which put the phone above the Mac —
				// and the Mac is the one that pays off before the person stands up.
				const g = PROMINENCE_ORDER.indexOf(a.copy.prominence) - PROMINENCE_ORDER.indexOf(b.copy.prominence);
				return g !== 0 ? g : (a.copy.rank ?? 99) - (b.copy.rank ?? 99);
			});
	});

	// ── connect dispatch + modals ──
	let pairOpen = $state(false);
	let pairDeviceType = $state<"ios" | "mac">("ios");
	let pairDisplayName = $state("iPhone");
	let apikeyOpen = $state(false);
	let apikeySource = $state<SourceCatalogItem | null>(null);
	let chatOpen = $state(false);

	async function connect(source: SourceCatalogItem) {
		err = null;
		const intent = await connectIntent(source);
		switch (intent.kind) {
			case "pair":
				pairDeviceType = intent.deviceType;
				pairDisplayName = intent.displayName;
				pairOpen = true;
				break;
			case "chat_import":
				chatOpen = true;
				break;
			case "api_key":
				apikeySource = intent.source;
				apikeyOpen = true;
				break;
			case "oauth":
				// Browser: redirecting away. Tauri: system browser handles it and
				// the SPA stays mounted — refresh sources when the user returns.
				if (intent.external) reloadOnReturn(load);
				break;
			case "error":
				err = intent.message;
				break;
		}
	}

	function afterConnect() {
		void load();
		onConnected();
	}
	function deviceDone() {
		onDeviceReady?.();
		afterConnect();
	}
</script>

<div class="connect-world">
	<p class="lede">
		Connect the accounts and devices you want Virtues to read from. Each one stays on the box; nothing is sent to us.
		Start with one — add the rest whenever.
	</p>
	<Marginalia tone="receipt">read-only · everything stays on your box · nothing is sent to Virtues</Marginalia>

	{#if err}
		<div class="err">{err}</div>
	{/if}

	<div class="rows">
		{#each featured as { source, copy } (source.id)}
			{#if source.id === "mac" && localMac}
				<div class="device-block">
					<div class="device-head">
						<span class="device-icon"><Icon icon="ri:macbook-line" width="20" /></span>
						<span class="device-name">Set up {thisComputerLabel}</span>
					</div>
					<p class="device-why">{copy.why}</p>
					<CollectorPermissionCard onComplete={deviceDone} />
				</div>
			{:else}
				<SourceRow {source} {copy} connected={source.credential_count > 0} onConnect={() => connect(source)} />
			{/if}
		{/each}
	</div>

	{#if isWindows || isLinux}
		<p class="aside">Desktop collection for {thisComputerLabel} is coming — your phone, email, and chat history cover you for now.</p>
	{/if}

	<p class="aside">More — finances, notes, fitness, and the rest — wait for you in the app once you're set up.</p>
</div>

<DevicePairModal
	deviceType={pairDeviceType}
	displayName={pairDisplayName}
	open={pairOpen}
	onClose={() => {
		pairOpen = false;
		void load();
	}}
	onSuccess={() => {
		pairOpen = false;
		afterConnect();
	}}
/>

<ApiKeyConnectModal
	source={apikeySource}
	open={apikeyOpen}
	onClose={() => (apikeyOpen = false)}
	onSuccess={() => {
		apikeyOpen = false;
		afterConnect();
	}}
/>

<Modal open={chatOpen} onClose={() => { chatOpen = false; afterConnect(); }} title="Import chat history" width="md">
	{#snippet children()}
		<ChatImportCard />
	{/snippet}
</Modal>

<style>
	@reference "../../../../app.css";

	.lede {
		margin: 0 0 2rem;
		font-size: 1.0625rem;
		line-height: 1.6;
		color: var(--color-foreground-muted);
	}

	.device-block {
		position: relative;
		padding: 1.1rem 0;
		border-top: 1px solid var(--color-border-subtle);
	}
	.device-head {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}
	.device-icon {
		display: flex;
		height: 2.25rem;
		width: 2.25rem;
		align-items: center;
		justify-content: center;
		border-radius: 0.6rem;
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}
	.device-name {
		font-family: var(--font-serif);
		font-size: 1.05rem;
		color: var(--color-foreground);
	}
	.device-why {
		margin: 0.4rem 0 0.9rem;
		font-size: 0.9rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.aside {
		margin-top: 1.5rem;
		font-size: 0.85rem;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}

	.err {
		margin-bottom: 1rem;
		border-radius: 0.5rem;
		border: 1px solid color-mix(in srgb, var(--color-error) 20%, transparent);
		background: var(--color-error-subtle);
		padding: 0.6rem 0.8rem;
		font-size: 0.85rem;
		color: var(--color-error);
	}
</style>
