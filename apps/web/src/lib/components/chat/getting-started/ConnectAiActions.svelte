<!--
	Connect AI's controls, as one row of three short buttons. Sign in unfolds
	in place into a single hairline field with an arrow; the sending and
	waiting states are one line with a breathing dot. The same setup
	endpoints AccountGate used; polls until the box holds a key. Keys are
	never typed here: the endpoint form lives in Billing. Not skippable from
	here — the door is the skip.
-->
<script lang="ts">
	import { onDestroy } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { openExternal } from "$lib/tauri/bridge";
	import { setupLinkPoll, setupSubscribeStart, setupLoginStart, setByoKey } from "$lib/api/client";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import SudoModal from "$lib/components/SudoModal.svelte";

	type Mode = "choose" | "login" | "sending" | "waiting" | "subscribe" | "endpoint" | "saving";
	let mode = $state<Mode>("choose");
	let email = $state("");
	let error = $state<string | null>(null);
	let checkoutUrl = $state<string | null>(null);
	let field = $state<HTMLInputElement | null>(null);
	let timer: ReturnType<typeof setInterval> | null = null;

	const valid = $derived(/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim()));

	$effect(() => {
		if (mode === "login") field?.focus();
	});

	function stop() {
		if (timer) clearInterval(timer);
		timer = null;
	}
	function poll() {
		stop();
		timer = setInterval(async () => {
			try {
				const d = await setupLinkPoll<{ status: string }>();
				if (d.status === "ready") {
					stop();
					await gettingStarted.refresh();
				} else if (d.status === "expired" || d.status === "none") {
					stop();
					mode = "choose";
					error = "That link expired. Start again.";
				}
			} catch {
				/* next tick */
			}
		}, 3000);
	}
	const UNREACHABLE =
		"Couldn't reach the Virtues billing service. Check your server's internet connection and try again.";
	async function subscribe() {
		error = null;
		try {
			const d = await setupSubscribeStart<{ verification_uri_complete?: string; verification_uri?: string }>();
			checkoutUrl = d.verification_uri_complete || d.verification_uri || null;
			mode = "subscribe";
			if (checkoutUrl) void openExternal(checkoutUrl);
			poll();
		} catch {
			error = UNREACHABLE;
		}
	}
	async function login() {
		if (!valid || mode === "sending") return;
		error = null;
		mode = "sending";
		try {
			const d = await setupLoginStart<{ status: string }>(email.trim());
			if (d.status === "sent") {
				mode = "waiting";
				poll();
			} else {
				mode = "login";
				if (d.status === "no_account") error = "No Virtues account on that email yet.";
				else if (d.status === "rate_limited") error = "Too many attempts for that email. Try again in an hour.";
				else error = "That didn't go through. Try again.";
			}
		} catch {
			mode = "login";
			error = UNREACHABLE;
		}
	}
	function back() {
		stop();
		error = null;
		mode = "choose";
	}
	onDestroy(stop);

	// An endpoint of your own, in place. The same contract Billing's form
	// speaks: a URL that answers OpenAI-style chat completions with a bearer
	// token, the key, and optionally what the endpoint calls our chat model.
	// The save is sudo-gated (change_byo_key): the modal mints the request
	// and hands back its id on approval at the server's command line.
	let endpointUrl = $state("");
	let apiKey = $state("");
	let chatModel = $state("");
	let showSudo = $state(false);
	let urlField = $state<HTMLInputElement | null>(null);
	const endpointValid = $derived(/^https?:\/\/\S+/.test(endpointUrl.trim()) && apiKey.trim().length > 0);
	$effect(() => {
		if (mode === "endpoint") urlField?.focus();
	});
	function startEndpointSave() {
		if (!endpointValid) return;
		error = null;
		showSudo = true;
	}
	async function saveEndpoint(sudoRequestId: string) {
		mode = "saving";
		try {
			const models = chatModel.trim() ? { chat: chatModel.trim() } : {};
			await setByoKey({
				sudo_request_id: sudoRequestId,
				endpoint_url: endpointUrl.trim(),
				api_key: apiKey,
				models,
			});
			apiKey = "";
			await gettingStarted.refresh();
		} catch (e) {
			mode = "endpoint";
			error = e instanceof Error ? e.message : "That didn't save. Try again.";
		}
	}
</script>

{#if mode === "choose"}
	<div class="row rise">
		<button type="button" class="btn" onclick={subscribe}>Subscribe · $20/mo</button>
		<button type="button" class="btn quiet" onclick={() => (mode = "login")}>Sign in</button>
		<button type="button" class="btn quiet" onclick={() => (mode = "endpoint")}>Own endpoint</button>
	</div>
{:else if mode === "login" || mode === "sending"}
	<form
		class="signin rise"
		class:sending={mode === "sending"}
		onsubmit={(e) => {
			e.preventDefault();
			void login();
		}}
	>
		<label class="field-wrap">
			<span class="sr-only">Email</span>
			<input
				class="field"
				type="email"
				autocomplete="email"
				spellcheck="false"
				placeholder="you@example.com"
				bind:value={email}
				bind:this={field}
				disabled={mode === "sending"}
			/>
		</label>
		<button type="submit" class="go" disabled={!valid || mode === "sending"} aria-label="Send the sign-in link">
			<Icon icon="ri:arrow-right-line" width="16" />
		</button>
		<button type="button" class="back" onclick={back}>Back</button>
	</form>
{:else if mode === "endpoint" || mode === "saving"}
	<form
		class="endpoint rise"
		class:sending={mode === "saving"}
		onsubmit={(e) => {
			e.preventDefault();
			startEndpointSave();
		}}
	>
		<p class="help">
			Any endpoint that answers OpenAI-style chat completions with a bearer token: a gateway such as Vercel AI Gateway or OpenRouter, a provider's own API, or a local Ollama. The key is stored encrypted on your server, and saving it asks for an approval at the server's command line.
		</p>
		<input
			class="field"
			type="url"
			spellcheck="false"
			placeholder="https://ai-gateway.vercel.sh/v1/chat/completions"
			bind:value={endpointUrl}
			bind:this={urlField}
			disabled={mode === "saving"}
		/>
		<input
			class="field"
			type="password"
			autocomplete="off"
			placeholder="API key"
			bind:value={apiKey}
			disabled={mode === "saving"}
		/>
		<input
			class="field"
			type="text"
			spellcheck="false"
			placeholder="Chat model id, if your endpoint names it differently (optional)"
			bind:value={chatModel}
			disabled={mode === "saving"}
		/>
		<div class="row">
			<button type="submit" class="btn" disabled={!endpointValid || mode === "saving"}>
				{mode === "saving" ? "Saving…" : "Save the endpoint"}
			</button>
			<button type="button" class="back" onclick={back}>Back</button>
		</div>
	</form>
	<SudoModal
		bind:show={showSudo}
		action="change_byo_key"
		title="Save your endpoint"
		description="Every AI call will route through this endpoint. Confirm at the server's command line."
		onApproved={saveEndpoint}
	/>
{:else if mode === "waiting"}
	<p class="line rise">
		<span class="dot" aria-hidden="true"></span>
		A sign-in link is in your email. This picks up on its own once you have used it.
		<button type="button" class="back" onclick={back}>Back</button>
	</p>
{:else if mode === "subscribe"}
	<p class="line rise">
		<span class="dot" aria-hidden="true"></span>
		Finish in the window that opened. This picks up on its own after checkout.
		{#if !checkoutUrl}<span class="muted">No window? Start again.</span>{/if}
		<button type="button" class="back" onclick={back}>Back</button>
	</p>
{/if}
{#if error}<p class="error rise">{error}</p>{/if}

<style>
	/* One motion, from-only: things arrive; nothing waits to leave. */
	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(5px);
		}
	}
	@keyframes breathe {
		from {
			opacity: 0.2;
		}
	}
	.rise {
		animation: rise 0.32s cubic-bezier(0.2, 0.7, 0.2, 1) both;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.btn {
		font: inherit;
		font-size: 0.875rem;
		padding: 0.45rem 0.95rem;
		border-radius: 999px;
		border: 1px solid var(--color-foreground);
		background: var(--color-foreground);
		color: var(--color-background);
		cursor: pointer;
		transition:
			border-color 0.18s ease,
			background-color 0.18s ease;
	}
	.btn.quiet {
		background: transparent;
		color: var(--color-foreground);
		border-color: var(--color-border);
	}
	.btn.quiet:hover {
		border-color: var(--color-foreground);
	}

	/* Sign in, unfolded: one hairline, one arrow. */
	.signin {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		max-width: 26rem;
	}
	.signin.sending {
		opacity: 0.6;
	}
	.field-wrap {
		flex: 1;
		min-width: 0;
	}
	.field {
		width: 100%;
		font: inherit;
		font-size: 1.0625rem;
		padding: 0.4rem 0;
		border: 0;
		border-bottom: 1px solid var(--color-border);
		border-radius: 0;
		background: transparent;
		color: var(--color-foreground);
		outline: none;
		transition: border-color 0.2s ease;
	}
	.field::placeholder {
		color: var(--color-foreground-subtle);
	}
	.field:focus {
		border-bottom-color: var(--color-foreground);
	}
	.go {
		flex: none;
		width: 2.125rem;
		height: 2.125rem;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 999px;
		border: 1px solid var(--color-foreground);
		background: var(--color-foreground);
		color: var(--color-background);
		cursor: pointer;
		transition:
			opacity 0.18s ease,
			transform 0.18s ease;
	}
	.go:disabled {
		opacity: 0.25;
		cursor: default;
	}
	.go:not(:disabled):hover {
		transform: translateX(1px);
	}
	.back {
		flex: none;
		background: none;
		border: 0;
		padding: 0.25rem 0;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}
	.back:hover {
		color: var(--color-foreground);
	}

	/* An endpoint of your own: three hairline fields, one paragraph. */
	.endpoint {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		max-width: 34rem;
	}
	.endpoint.sending {
		opacity: 0.6;
	}
	.help {
		margin: 0 0 0.25rem;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}

	/* Waiting: a line and a breathing dot. */
	.line {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem 0.75rem;
		margin: 0;
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}
	.dot {
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 999px;
		background: var(--color-foreground);
		animation: breathe 1.3s ease-in-out infinite alternate;
		flex: none;
	}
	.muted {
		color: var(--color-foreground-subtle);
	}
	.error {
		margin: 0.5rem 0 0;
		font-size: 0.875rem;
		color: var(--color-error, #9a2b2e);
	}
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
	}
</style>
