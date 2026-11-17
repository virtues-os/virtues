<script lang="ts">
	/**
	 * Settings → Billing → Bring your own AI, a page of its own (the form is
	 * longer than a row).
	 *
	 * The escape hatch from the Virtues wallet: `BearerClient` diverts every
	 * `/v1/ai/*` call, streaming and not, so chat, compaction, day summaries,
	 * image generation and transcription all leave by the owner's endpoint.
	 *
	 * The copy here is load-bearing and has been wrong in both directions. If
	 * you change it, verify against api/settings_byo.rs and
	 * virtues_api/client.rs, not grep: the routing reaches the credential
	 * through `load_byo_credential` and never names `BYO_SOURCE_ID`.
	 *
	 * A credential is a URL and a key. There is no provider picker: one
	 * contract (OpenAI-style /chat/completions with a bearer token), so only
	 * the address varies. Example URLs are copy in the markup, never options:
	 * a stale doc line is wrong, a stale option is a broken feature. Plan:
	 * agents/plan/byo-ai-plan.md.
	 *
	 * Save and Delete carry the app's owner proof (`setByoKey`,
	 * `deleteByoKey`); the app signs it, so neither has an extra step.
	 */
	import { onMount } from 'svelte';
	import { Button, Input, Badge, Page } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import { getByoKey, setByoKey, deleteByoKey } from '$lib/api/client';
	import { toast } from 'svelte-sonner';
	import { confirmAction } from '$lib/stores/dialog.svelte';

	type ByoStatus = {
		configured: boolean;
		/** Legacy label on credentials saved before the picker was removed. */
		provider: string | null;
		/** Slot name → the model id on the owner's endpoint. */
		models: Record<string, string>;
		default_model: string | null;
		endpoint_url: string | null;
		/** Tokens, when the owner set one for a small or local model. */
		context_window: number | null;
		created_at: string | null;
	};

	/** Host of an endpoint URL, for display. Falls back to the raw string. */
	function byoHost(url: string): string {
		try {
			return new URL(url).host;
		} catch {
			return url;
		}
	}

	/**
	 * The slot map form. Keys match the server's slot names exactly. `note`
	 * is where judgment about which model suits a role lives: model
	 * landscapes shift faster than servers update, so guidance belongs where
	 * a copy edit can fix it.
	 */
	const SLOT_FIELDS = [
		{ key: 'chat', label: 'Chat', placeholder: 'provider/model', note: '' },
		{ key: 'coding', label: 'Coding', placeholder: 'provider/model', note: '' },
		{
			key: 'lite',
			label: 'Lite',
			placeholder: 'provider/model',
			note: 'Titles, summaries, and background jobs. High volume, so favor something cheap and quick.',
		},
		{ key: 'image', label: 'Image', placeholder: 'provider/model', note: '' },
		{
			key: 'omni',
			label: 'Audio',
			placeholder: 'provider/model',
			note: 'Transcription, and usually the largest line item. The model has to hear muffled, in-pocket audio and make sense of the scene. Speech-to-text models will not do, and most chat models refuse audio outright. In practice a Gemini Flash model is the one workable choice.',
		},
	] as const;

	/**
	 * One entry per slot, seeded blank. Binding `bind:value` to a missing key
	 * binds to `undefined`, which Svelte 5 refuses mid-render and leaves the
	 * page stuck on loading; seeding every slot prevents that.
	 */
	function blankSlots(): Record<string, string> {
		return Object.fromEntries(SLOT_FIELDS.map((f) => [f.key, '']));
	}

	let status = $state<ByoStatus | null>(null);
	let loading = $state(true);
	let loadError = $state<string | null>(null);

	let apiKey = $state('');
	let endpointUrl = $state('');
	/** As typed; blank means "let the server look it up". */
	let contextWindow = $state('');
	let models = $state<Record<string, string>>(blankSlots());
	/** A save or a removal is in flight. A save asks the endpoint first. */
	let busy = $state(false);

	async function load() {
		loading = true;
		loadError = null;
		try {
			status = await getByoKey<ByoStatus>();
			// Prefill so editing changes the map instead of clearing it, merged
			// over a blank map so an omitted slot stays bound to ''.
			models = { ...blankSlots(), ...(status?.models ?? {}) };
			contextWindow = status?.context_window ? String(status.context_window) : '';
		} catch (e) {
			loadError = e instanceof Error ? e.message : "Your server couldn't read your provider settings.";
		} finally {
			loading = false;
		}
	}

	onMount(load);

	async function save() {
		if (!endpointUrl.trim()) {
			toast.error('Enter the endpoint URL first.');
			return;
		}
		if (!apiKey.trim()) {
			toast.error('Enter the API key first.');
			return;
		}
		busy = true;
		try {
			// No `provider`: the server takes the URL as given. Blank rows are
			// omitted, not sent as "": an absent slot means "your endpoint uses
			// our id".
			const sent = Object.fromEntries(
				Object.entries(models)
					.map(([k, v]) => [k, (v ?? '').trim()])
					.filter(([, v]) => v.length > 0),
			);
			const window = Number.parseInt(contextWindow, 10);
			if (contextWindow.trim() && !(window >= 1000 && window <= 10_000_000)) {
				toast.error('Context window must be a number of tokens, from 1,000 to 10,000,000.');
				return;
			}
			await setByoKey({
				api_key: apiKey,
				endpoint_url: endpointUrl,
				models: sent,
				...(Number.isFinite(window) && window > 0 ? { context_window: window } : {}),
			});
			toast.success('Key saved. AI calls now go to your endpoint.');
			apiKey = '';
			await load();
		} catch (e) {
			toast.error('Could not save the key', { description: e instanceof Error ? e.message : 'Unknown error' });
		} finally {
			busy = false;
		}
	}

	async function remove() {
		const ok = await confirmAction({
			title: 'Remove your AI key?',
			body: 'AI calls go back to the Virtues subscription. You can add a key again at any time.',
			confirmLabel: 'Remove',
			danger: true,
		});
		if (!ok) return;
		busy = true;
		try {
			await deleteByoKey();
		} catch (e) {
			toast.error('Could not remove the key', { description: e instanceof Error ? e.message : 'Unknown error' });
			return;
		} finally {
			busy = false;
		}
		await load();
		// Servers on older releases answer ok even when the write failed, so
		// the status read afterwards decides which message is true.
		if (!loadError && status?.configured) {
			toast.error('Could not remove the key', {
				description: 'Your server still has it, and AI calls still go to your endpoint. Try again.',
			});
			return;
		}
		toast.success('Key removed. AI calls are back on the subscription.', {
			description: 'Your provider still accepts the key until you revoke it there.',
		});
	}
</script>

<Page
	title="Bring your own AI"
	description={status?.configured && status.endpoint_url
		? `Every AI call goes to ${byoHost(status.endpoint_url)}${status.default_model ? `, using ${status.default_model} by default` : ''}.`
		: 'Send every AI call to an endpoint of your own instead of the subscription.'}
	maxWidth="reading"
>
	<p class="prose-note">
		Any endpoint that speaks OpenAI-style chat completions with a bearer token works. That covers
		gateways like Vercel AI Gateway, OpenRouter, and LiteLLM, a provider's own API, and a local
		server such as Ollama or llama.cpp. AWS Bedrock signs requests instead of taking a key, so it
		needs a gateway in front. Your key is stored encrypted on your server. To change it, the Virtues
		app on one of your paired devices signs the request with that device's own key. Maps, web
		search, photos, and bank connections are not AI calls and stay on the subscription.
	</p>
	<p class="prose-note warn">
		One caution. A key from your employer sends your personal life through systems they can read.
		Your own key lets you choose the vendor and pay the bill directly, but it adds no privacy.
		Whoever runs the endpoint still sees every call.
	</p>

	{#if loading}
		<p class="lede">Checking for a key…</p>
	{:else if loadError}
		<p class="note note-error">
			Your server couldn't read your provider settings.
			<span class="error-code">{loadError}</span>
		</p>
	{:else if status?.configured}
		<div class="panel">
			<div class="panel-head">
				<div class="panel-mark"><Icon icon="mynaui:key" /></div>
				<div class="panel-body">
					<div class="panel-title">AI calls go to your endpoint</div>
					<div class="panel-facts">
						{#if status.endpoint_url}
							<span>Sending to <Badge>{byoHost(status.endpoint_url)}</Badge></span>
							<span class="mono">{status.endpoint_url}</span>
						{/if}
						{#if status.default_model}
							<span>Default model <code>{status.default_model}</code></span>
						{/if}
					</div>
					<p class="panel-foot">
						Every AI call goes straight from your server to your provider. The subscription isn't in
						the path, and we don't charge it.
					</p>
				</div>
				<Button variant="ghost" disabled={busy} onclick={remove}>
					<Icon icon="mynaui:x-circle" />
					Remove
				</Button>
			</div>
		</div>
		<div class="panel">
			<div class="panel-title">Replace the key</div>
			{@render keyForm()}
			<div class="panel-actions">
				<Button variant="primary" loading={busy} onclick={save}>Save new key</Button>
			</div>
		</div>
	{:else}
		<div class="panel">
			<div class="panel-title">No key set</div>
			<p class="panel-lede">
				Everything runs on the Virtues subscription right now. Set an endpoint to send AI calls there
				instead.
			</p>
			{@render keyForm()}
			<div class="panel-actions">
				<Button variant="primary" loading={busy} onclick={save}>Save key</Button>
			</div>
		</div>
	{/if}
</Page>

{#snippet keyForm()}
	<div class="space-y-3">
		<div>
			<label class="block text-xs text-foreground-muted mb-1" for="byo-url">Endpoint URL</label>
			<Input id="byo-url" bind:value={endpointUrl} placeholder="https://ai-gateway.vercel.sh/v1/chat/completions" />
			<p class="text-xs text-foreground-muted mt-1.5">
				Any endpoint that speaks OpenAI-style <code>/chat/completions</code> with a bearer token.
			</p>
		</div>
		<div>
			<label class="block text-xs text-foreground-muted mb-1" for="byo-context-window">Context window (tokens)</label>
			<Input id="byo-context-window" bind:value={contextWindow} placeholder="Optional, e.g. 32000" />
			<p class="text-xs text-foreground-muted mt-1.5">
				Set this for a local or small model, so long chats are summarized before they overflow it.
				Blank: your server goes by the model id, and assumes 200k when it can't tell.
			</p>
		</div>
		<div>
			<label class="block text-xs text-foreground-muted mb-1" for="byo-key">API key</label>
			<Input id="byo-key" type="password" bind:value={apiKey} placeholder="sk-…" />
		</div>
		<div class="pt-1">
			<div class="text-xs font-medium mb-1">What your endpoint calls each model</div>
			<p class="text-xs text-foreground-muted mb-3">
				Optional. If your endpoint names models differently, say what it calls each of these. A blank
				row uses the same id Virtues does.
			</p>
			<div class="space-y-2">
				{#each SLOT_FIELDS as slot (slot.key)}
					<div class="flex flex-col sm:flex-row sm:items-center gap-1 sm:gap-3">
						<label class="text-xs text-foreground-muted sm:w-28 sm:shrink-0 sm:text-right" for="byo-model-{slot.key}">{slot.label}</label>
						<div class="flex-1">
							<Input id="byo-model-{slot.key}" bind:value={models[slot.key]} placeholder={slot.placeholder} />
						</div>
					</div>
					{#if slot.note}
						<p class="text-xs text-foreground-muted sm:ml-[7.75rem] -mt-1">{slot.note}</p>
					{/if}
				{/each}
			</div>
		</div>
	</div>
{/snippet}

<style>
	.lede {
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0;
	}
	.mono {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-variant-numeric: tabular-nums;
	}
	.prose-note {
		font-size: 12px;
		line-height: 1.6;
		color: var(--color-foreground-muted);
		margin: 0 0 12px;
		max-width: 68ch;
	}
	.prose-note.warn {
		color: var(--color-warning);
	}
	.note {
		font-size: 12px;
		line-height: 1.5;
		margin: 8px 0 0;
		padding: 8px 12px;
		border-radius: var(--radius-md);
		border: 1px solid transparent;
		max-width: 60ch;
	}
	.note-error {
		color: var(--color-error);
		border-color: color-mix(in srgb, var(--color-error) 22%, transparent);
		background: var(--color-error-subtle);
	}
	.error-code {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.85em;
		opacity: 0.72;
	}
	/* Places you act. */
	.panel {
		margin-top: 12px;
		padding: 16px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.panel-head {
		display: flex;
		align-items: flex-start;
		gap: 12px;
	}
	.panel-mark {
		flex-shrink: 0;
		width: 38px;
		height: 38px;
		border-radius: var(--radius-lg);
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(in srgb, var(--color-success) 10%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-success) 30%, transparent);
		color: var(--color-success);
	}
	.panel-body {
		flex: 1;
		min-width: 0;
	}
	.panel-title {
		font-size: 14px;
		font-weight: 500;
		color: var(--color-foreground);
	}
	.panel-lede,
	.panel-foot {
		font-size: 12px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0;
	}
	.panel-facts {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 12px;
		font-size: 12px;
		color: var(--color-foreground-muted);
		margin-top: 4px;
	}
	.panel-actions {
		display: flex;
		justify-content: flex-end;
	}
	code {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.92em;
		background: var(--color-surface-elevated);
		padding: 0 4px;
		border-radius: var(--radius-sm);
	}
</style>
