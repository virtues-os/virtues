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
	import DevicePairModal from "$lib/components/sources/DevicePairModal.svelte";
	import ApiKeyConnectModal from "$lib/components/sources/ApiKeyConnectModal.svelte";
	import ChatImportCard from "$lib/components/onboarding/ChatImportCard.svelte";
	import CollectorPermissionCard from "$lib/components/onboarding/CollectorPermissionCard.svelte";
	import { connectIntent, reloadOnReturn } from "$lib/components/sources/connectDispatch";
	import {
		listSourceCatalog,
		listCredentials,
		getChatImportStatus,
		type SourceCatalogItem,
		type Credential,
		type ChatImportStatus,
	} from "$lib/api/client";
	import { isTauri, isMacOS, isWindows, isLinux, thisComputerLabel } from "$lib/utils/platform";
	import { copyFor, PROMINENCE_ORDER, type Prominence } from "./sources-copy";
	import SourceRow from "./SourceRow.svelte";

	interface Props {
		/** Called whenever a source connects, so the shell can refresh derived state. */
		onConnected: () => void;
		/** Called the moment the local Mac collector finishes (optimistic). */
		onDeviceReady?: () => void;
	}

	let { onConnected, onDeviceReady }: Props = $props();

	let catalog = $state<SourceCatalogItem[]>([]);
	let credentials = $state<Credential[]>([]);
	let chatImport = $state<ChatImportStatus | null>(null);
	let err = $state<string | null>(null);

	const localMac = $derived(isTauri && isMacOS);

	async function load() {
		try {
			const [src, creds] = await Promise.all([listSourceCatalog(), listCredentials()]);
			catalog = src;
			credentials = creds;
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		}
		// Best-effort: chat-import mints no credential, so its connected state
		// comes from the imported rows. Absence of the endpoint (older box)
		// just means the row keeps its Connect button.
		try {
			chatImport = await getChatImportStatus();
		} catch {
			chatImport = null;
		}
	}

	const PROVIDER_NAMES: Record<string, string> = {
		claude: "Claude",
		chatgpt: "ChatGPT",
		gemini: "Gemini",
	};

	/** Is this source connected, and what is the receipt line under it? */
	function statusFor(source: SourceCatalogItem): { connected: boolean; detail: string | null } {
		if (source.id === "chat_import") {
			if (!chatImport || chatImport.messages === 0) return { connected: false, detail: null };
			const from = chatImport.providers.map((p) => PROVIDER_NAMES[p] ?? p).join(", ");
			return {
				connected: true,
				detail: `${chatImport.messages.toLocaleString()} messages across ${chatImport.conversations.toLocaleString()} conversations${from ? ` · ${from}` : ""}`,
			};
		}
		if (source.credential_count > 0) {
			// The receipt: which accounts. The credential name is the account's
			// email when the provider gave us one (see source_auth.rs). When the
			// provider gave us nothing, every credential shares the generic
			// fallback name — a count reads better than a stutter.
			const names = credentials
				.filter((c) => c.provider === source.id && c.is_active)
				.map((c) => c.name);
			const unique = [...new Set(names)];
			const detail =
				unique.length < names.length
					? `${names.length} accounts`
					: unique.join(" · ") || null;
			return { connected: true, detail };
		}
		return { connected: false, detail: null };
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
		// A connect started here must land back here — the callback's default
		// 302 into /sources dumps the person into the app mid-onboarding.
		const intent = await connectIntent(source, { next: "/onboarding/sources" });
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
	<!-- NO PROSE. The lede, the sidenote/Marginalia, the mono receipt line and
	     the per-row second sentences all left in successive cuts (last:
	     2026-08-21). The list is the screen: icon, name, one line of concrete
	     nouns, a quiet Connect. Whatever needs arguing was the letter's job. -->
	{#if err}
		<div class="err">{err}</div>
	{/if}

	<div class="rows">
		{#each featured as { source, copy } (source.id)}
			{#if source.id === "mac" && localMac}
				<div class="device-block">
					<div class="device-head">
						<span class="device-name">Set up {thisComputerLabel}</span>
					</div>
					<p class="device-why">{copy.why}</p>
					<CollectorPermissionCard onComplete={deviceDone} />
				</div>
			{:else}
				{@const status = statusFor(source)}
				<SourceRow
					{source}
					{copy}
					connected={status.connected}
					detail={status.detail}
					onConnect={() => connect(source)}
				/>
			{/if}
		{/each}
	</div>

	{#if isWindows || isLinux}
		<p class="aside">Desktop collection for {thisComputerLabel} is coming — your phone, email, and chat history cover you for now.</p>
	{/if}
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
