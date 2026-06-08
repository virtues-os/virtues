<!--
  Settings → AI Provider Key (BYO).

  The escape hatch from the Virtues wallet. When a key is set here, every
  chat call routes box → provider directly, bypassing virtues-api entirely.
  Virtues is no longer in the inference path.

  Save and Delete are both sudo-gated (`change_byo_key` is one of the four
  locked sensitive actions); the SudoModal handles the prompt + CLI
  approval round-trip.
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page, Button, Input, Badge, SudoModal, EmptyState, LoadingState, ErrorState } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";
	import { toast } from "svelte-sonner";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type Status = {
		configured: boolean;
		provider: string | null;
		default_model: string | null;
		endpoint_url: string | null;
		created_at: string | null;
	};

	let status = $state<Status | null>(null);
	let loading = $state(true);
	let loadError = $state<string | null>(null);

	// Form state
	let provider = $state<"openai" | "anthropic" | "xai" | "google" | "custom">("openai");
	let apiKey = $state("");
	let endpointUrl = $state("");
	let defaultModel = $state("");

	// Sudo modal coordination — we mint a sudo request, then on approval we
	// fire the actual save/delete with the approved request id.
	let showSudoSave = $state(false);
	let showSudoDelete = $state(false);

	onMount(load);

	async function load() {
		loading = true;
		loadError = null;
		try {
			const resp = await fetch("/api/settings/byo-key");
			if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
			status = (await resp.json()) as Status;
		} catch (e) {
			loadError = e instanceof Error ? e.message : "Failed to load BYO status";
		} finally {
			loading = false;
		}
	}

	function startSave() {
		if (!apiKey.trim()) {
			toast.error("Paste an API key first");
			return;
		}
		if (provider === "custom" && !endpointUrl.trim()) {
			toast.error("Custom provider requires an endpoint URL");
			return;
		}
		showSudoSave = true;
	}

	async function performSave(sudoRequestId: string) {
		try {
			const resp = await fetch("/api/settings/byo-key", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					sudo_request_id: sudoRequestId,
					provider,
					api_key: apiKey,
					endpoint_url: endpointUrl || null,
					default_model: defaultModel || null,
				}),
			});
			if (!resp.ok) {
				const data = await resp.json().catch(() => ({}));
				throw new Error(data.error ?? `HTTP ${resp.status}`);
			}
			toast.success("BYO key saved");
			apiKey = "";
			await load();
		} catch (e) {
			toast.error("Save failed", {
				description: e instanceof Error ? e.message : "Unknown error",
			});
		}
	}

	function startDelete() {
		showSudoDelete = true;
	}

	async function performDelete(sudoRequestId: string) {
		try {
			const resp = await fetch("/api/settings/byo-key", {
				method: "DELETE",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ sudo_request_id: sudoRequestId }),
			});
			if (!resp.ok) {
				const data = await resp.json().catch(() => ({}));
				throw new Error(data.error ?? `HTTP ${resp.status}`);
			}
			toast.success("BYO key removed — chat is back on your Virtues wallet");
			await load();
		} catch (e) {
			toast.error("Delete failed", {
				description: e instanceof Error ? e.message : "Unknown error",
			});
		}
	}
</script>

<Page>
	<div class="px-6 py-6 max-w-2xl mx-auto w-full">
		<div class="mb-6">
			<h1 class="text-2xl font-semibold tracking-tight">AI Provider Key</h1>
			<p class="text-sm text-foreground-muted mt-1">
				Bring your own OpenAI / Anthropic / xAI / Google / custom key. When
				set, every chat call goes from your box directly to the provider —
				the Virtues cloud is out of the path entirely. The Virtues
				subscription still works in the background; you can switch back
				anytime by deleting the key here.
			</p>
		</div>

		{#if loading}
			<LoadingState />
		{:else if loadError}
			<ErrorState message={loadError} />
		{:else if status?.configured}
			<div class="rounded-lg border border-border bg-surface p-5 mb-6">
				<div class="flex items-start gap-3">
					<div class="flex-shrink-0 w-10 h-10 rounded-lg bg-success/10 border border-success/30 flex items-center justify-center">
						<Icon icon="ri:key-line" class="text-success" />
					</div>
					<div class="flex-1 min-w-0">
						<div class="font-medium">BYO key active</div>
						<div class="text-xs text-foreground-muted mt-1 flex flex-wrap gap-x-3 gap-y-1">
							<span>Provider: <Badge>{status.provider ?? "?"}</Badge></span>
							{#if status.default_model}
								<span>Model: <code class="text-xs">{status.default_model}</code></span>
							{/if}
							{#if status.endpoint_url}
								<span>Endpoint: <code class="text-xs">{status.endpoint_url}</code></span>
							{/if}
						</div>
						<p class="text-xs text-foreground-muted mt-2">
							Virtues is not in your inference path. Wallet & top-up are
							inactive while this key is set.
						</p>
					</div>
					<Button variant="ghost" onclick={startDelete}>
						<Icon icon="ri:close-circle-line" />
						Remove
					</Button>
				</div>
			</div>

			<div class="rounded-lg border border-border bg-surface p-5">
				<div class="font-medium mb-3">Replace the key</div>
				{@render keyForm()}
				<div class="flex justify-end mt-4">
					<Button variant="primary" onclick={startSave}>Save new key</Button>
				</div>
			</div>
		{:else}
			<div class="rounded-lg border border-border bg-surface p-5">
				<div class="font-medium mb-1">No BYO key set</div>
				<p class="text-xs text-foreground-muted mb-4">
					Currently routing through the Virtues wallet ($20/mo subscription
					+ usage). Paste a provider key below to swap.
				</p>
				{@render keyForm()}
				<div class="flex justify-end mt-4">
					<Button variant="primary" onclick={startSave}>Save key</Button>
				</div>
			</div>
		{/if}

		<div class="text-xs text-foreground-muted mt-6">
			<strong>v1 supports OpenAI-compatible APIs.</strong> For Anthropic-native
			or Google-native shapes, point BYO at a translation proxy like
			LiteLLM or OpenRouter and choose
			<code class="bg-surface-alt px-1 rounded">custom</code> with their
			endpoint URL.
		</div>
	</div>
</Page>

{#snippet keyForm()}
	<div class="space-y-3">
		<div>
			<label class="block text-xs text-foreground-muted mb-1" for="byo-provider"
				>Provider</label
			>
			<select
				id="byo-provider"
				bind:value={provider}
				class="w-full rounded border border-border bg-surface px-3 py-2 text-sm"
			>
				<option value="openai">OpenAI</option>
				<option value="anthropic">Anthropic (via translation proxy)</option>
				<option value="xai">xAI</option>
				<option value="google">Google (via translation proxy)</option>
				<option value="custom">Custom (OpenAI-compatible)</option>
			</select>
		</div>
		<div>
			<label class="block text-xs text-foreground-muted mb-1" for="byo-key"
				>API key</label
			>
			<Input
				id="byo-key"
				type="password"
				bind:value={apiKey}
				placeholder="sk-…"
			/>
		</div>
		{#if provider === "custom"}
			<div>
				<label class="block text-xs text-foreground-muted mb-1" for="byo-url"
					>Endpoint URL</label
				>
				<Input
					id="byo-url"
					bind:value={endpointUrl}
					placeholder="https://api.example.com/v1/chat/completions"
				/>
			</div>
		{/if}
		<div>
			<label
				class="block text-xs text-foreground-muted mb-1"
				for="byo-model">Default model (optional)</label
			>
			<Input
				id="byo-model"
				bind:value={defaultModel}
				placeholder="gpt-4o, claude-3-5-sonnet-latest, …"
			/>
		</div>
	</div>
{/snippet}

<SudoModal
	bind:show={showSudoSave}
	action="change_byo_key"
	title="Save BYO AI key"
	description="Sensitive action — every future chat call will route through this key. Confirm at the box CLI."
	onApproved={performSave}
/>

<SudoModal
	bind:show={showSudoDelete}
	action="change_byo_key"
	title="Remove BYO AI key"
	description="Sensitive action — chat will switch back to the Virtues wallet. Confirm at the box CLI."
	onApproved={performDelete}
/>
