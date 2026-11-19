<script lang="ts">
	/**
	 * Settings · Models and spending · Your own AI provider, a page of its own
	 * (the form is longer than a row). It sits under Models and spending
	 * because it decides where every AI call runs.
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
	import { Button, Input } from '$lib';
	import SettingsPage from './SettingsPage.svelte';
	import SettingRow from './SettingRow.svelte';
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
	 * The slot map form. Keys are the server's slot names (`ModelSlot::as_str`
	 * in the registry, which is also how the server hands the map back),
	 * labelled as Models and spending labels them. `note` is where judgment
	 * about which model suits a role lives: model landscapes shift faster
	 * than servers update, so guidance belongs where a copy edit can fix it.
	 */
	const SLOT_FIELDS = [
		{ key: 'standard', label: 'Chat', note: '' },
		{ key: 'deep', label: 'Deep research', note: '' },
		{ key: 'lite', label: 'Background work', note: 'Titles, summaries and applets. These run often, so pick something cheap and quick.' },
		{ key: 'image', label: 'Pictures', note: '' },
		{
			key: 'omni',
			label: 'Audio',
			note: 'Transcription, usually the largest cost. The model has to understand muffled, in-pocket audio, which speech-to-text models and most chat models cannot do.',
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
			endpointUrl = status?.endpoint_url ?? '';
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
			title: 'Stop using your own provider?',
			body: 'Your server deletes the key, and AI calls go back through your Virtues subscription. You can add one again at any time.',
			confirmLabel: 'Stop using it',
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

<SettingsPage
	title="Your own AI provider"
	lede={status?.configured && status.endpoint_url
		? `Every AI call goes to ${byoHost(status.endpoint_url)}.`
		: "Send your server's AI calls to a provider and key of your own, instead of through Virtues."}
	back={{ route: '/virtues/assistant/spending', label: 'Models and spending' }}
>
	<p class="intro">
		Any endpoint that speaks the OpenAI-style chat API works, such as OpenRouter, a provider's own API, or a model on
		your network. Your key is stored encrypted on your server. Web search, places and bank data still go through your
		subscription.
	</p>
	<p class="intro">
		Whoever runs the endpoint sees every call. A key from your employer sends your life through systems they can read.
	</p>

	{#if loading}
		<p class="intro">Checking for a provider…</p>
	{:else if loadError}
		<p class="note note-error">
			Your server couldn't read your provider settings.
			<span class="error-code">{loadError}</span>
		</p>
	{:else}
		{#if status?.configured}
			<section class="group">
				<h2 class="settings-label">In use now</h2>
				<SettingRow id="byo-now" label="Sending to">
					<div class="now">
						<span class="mono">{status.endpoint_url ?? 'Your endpoint'}</span>
						<Button variant="danger" disabled={busy} onclick={remove}>Stop using it</Button>
					</div>
				</SettingRow>
				{#if status.default_model}
					<SettingRow id="byo-default-model" label="Chat model">
						<span class="mono">{status.default_model}</span>
					</SettingRow>
				{/if}
			</section>
		{/if}

		<section class="group">
			<h2 class="settings-label">{status?.configured ? 'Change it' : 'Your provider'}</h2>
			<SettingRow id="byo-url" label="Endpoint" forId="byo-url-input">
				<Input id="byo-url-input" bind:value={endpointUrl} placeholder="https://openrouter.ai/api/v1/chat/completions" />
			</SettingRow>
			<SettingRow
				id="byo-key"
				label="Key"
				forId="byo-key-input"
				help={status?.configured ? 'Enter the key again to change anything on this page.' : undefined}
			>
				<Input id="byo-key-input" type="password" bind:value={apiKey} placeholder="sk-…" />
			</SettingRow>
			<SettingRow
				id="byo-context-window"
				label="Context window"
				forId="byo-context-window-input"
				help="Optional, in tokens. Set it for a small or local model, so long chats are summarized before they fill it."
			>
				<div class="short"><Input id="byo-context-window-input" bind:value={contextWindow} placeholder="32000" /></div>
			</SettingRow>
		</section>

		<section class="group">
			<h2 class="settings-label">Model names</h2>
			<p class="intro">Only if your provider names models differently. A blank one uses the name Virtues does.</p>
			{#each SLOT_FIELDS as slot (slot.key)}
				<SettingRow id="byo-model-{slot.key}" label={slot.label} forId="byo-model-{slot.key}-input" help={slot.note || undefined}>
					<Input id="byo-model-{slot.key}-input" bind:value={models[slot.key]} placeholder="provider/model" />
				</SettingRow>
			{/each}
		</section>

		<div class="actions">
			<div><Button variant="primary" loading={busy} onclick={save}>Check and use it</Button></div>
			<p class="intro">
				Your server sends one short call to check the key before it switches. Changing it takes the Virtues app on one
				of your paired devices.
			</p>
		</div>
	{/if}
</SettingsPage>

<style>
	.intro {
		margin: 0 0 12px;
		max-width: 68ch;
		font-size: var(--text-small);
		line-height: 1.55;
		color: var(--color-foreground-muted);
	}
	.mono {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: var(--text-small);
		overflow-wrap: anywhere;
		color: var(--color-foreground);
	}
	.group {
		padding: 24px 0 8px;
	}
	/* A section heading, as on every Settings page: the app's serif h2. */
	.settings-label {
		font-family: var(--font-serif);
		font-size: var(--text-heading);
		font-weight: 400;
		line-height: 1.3;
		margin: 0 0 8px;
	}
	/* A field fills the row's answer column. */
	.group :global(.input-container) {
		width: 100%;
	}
	.now {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		width: 100%;
	}
	.short {
		width: 160px;
	}
	.actions {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 24px 0 8px;
	}
	.note {
		font-size: var(--text-caption);
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
</style>
