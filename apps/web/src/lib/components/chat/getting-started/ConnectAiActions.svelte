<!--
	Connect AI's controls: three doors in one row. Sign in unfolds in place
	into a line and an arrow; an endpoint of your own unfolds into three
	lines and the approval at the server's command line. Keys are typed here
	and nowhere else in the room, and never reach the transcript.
-->
<script lang="ts">
	import { onDestroy } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import SudoModal from "$lib/components/SudoModal.svelte";
	import { openExternal } from "$lib/tauri/bridge";
	import { setupLinkPoll, setupSubscribeStart, setupLoginStart, setByoKey } from "$lib/api/client";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import Act from "./ui/Act.svelte";
	import Choices from "./ui/Choices.svelte";
	import Line from "./ui/Line.svelte";
	import Waiting from "./ui/Waiting.svelte";

	type Mode = "choose" | "signin" | "sending" | "mailed" | "checkout" | "endpoint" | "saving";
	let mode = $state<Mode>("choose");
	let error = $state<string | null>(null);
	let timer: ReturnType<typeof setInterval> | null = null;

	let email = $state("");
	let endpointUrl = $state("");
	let apiKey = $state("");
	let chatModel = $state("");
	let showSudo = $state(false);
	let checkoutUrl = $state<string | null>(null);

	const emailValid = $derived(/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim()));
	const endpointValid = $derived(/^https?:\/\/\S+/.test(endpointUrl.trim()) && apiKey.trim().length > 0);

	const UNREACHABLE =
		"Couldn't reach the Virtues billing service. Check your server's internet connection and try again.";

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
	function back() {
		stop();
		error = null;
		mode = "choose";
	}

	async function subscribe() {
		error = null;
		try {
			const d = await setupSubscribeStart<{ verification_uri_complete?: string; verification_uri?: string }>();
			checkoutUrl = d.verification_uri_complete || d.verification_uri || null;
			mode = "checkout";
			if (checkoutUrl) void openExternal(checkoutUrl);
			poll();
		} catch {
			error = UNREACHABLE;
		}
	}
	async function signIn() {
		if (!emailValid || mode === "sending") return;
		error = null;
		mode = "sending";
		try {
			const d = await setupLoginStart<{ status: string }>(email.trim());
			if (d.status === "sent") {
				mode = "mailed";
				poll();
			} else {
				mode = "signin";
				if (d.status === "no_account") error = "No Virtues account on that email yet.";
				else if (d.status === "rate_limited") error = "Too many attempts for that email. Try again in an hour.";
				else error = "That didn't go through. Try again.";
			}
		} catch {
			mode = "signin";
			error = UNREACHABLE;
		}
	}
	async function saveEndpoint(sudoRequestId: string) {
		mode = "saving";
		try {
			await setByoKey({
				sudo_request_id: sudoRequestId,
				endpoint_url: endpointUrl.trim(),
				api_key: apiKey,
				models: chatModel.trim() ? { chat: chatModel.trim() } : {},
			});
			apiKey = "";
			await gettingStarted.refresh();
		} catch (e) {
			mode = "endpoint";
			error = e instanceof Error ? e.message : "That didn't save. Try again.";
		}
	}
	onDestroy(stop);
</script>

{#if mode === "choose"}
	<Choices>
		<Act variant="primary" onclick={subscribe}>Subscribe · $20/mo</Act>
		<Act onclick={() => (mode = "signin")}>Sign in</Act>
		<Act onclick={() => (mode = "endpoint")}>Own endpoint</Act>
	</Choices>
{:else if mode === "signin" || mode === "sending"}
	<form
		onsubmit={(e) => {
			e.preventDefault();
			void signIn();
		}}
	>
		<Choices>
			<Line bind:value={email} type="email" placeholder="you@example.com" autofocus disabled={mode === "sending"} />
			<Act type="submit" variant="primary" disabled={!emailValid || mode === "sending"}>
				{mode === "sending" ? "Sending…" : "Send a link"}
			</Act>
			<Act variant="plain" onclick={back}>Back</Act>
		</Choices>
	</form>
{:else if mode === "mailed"}
	<Choices>
		<Waiting>A sign-in link is in your email. This picks up on its own once you have used it.</Waiting>
		<Act variant="plain" onclick={back}>Back</Act>
	</Choices>
{:else if mode === "checkout"}
	<Choices>
		<Waiting>Finish in the window that opened. This picks up on its own after checkout.</Waiting>
		<Act variant="plain" onclick={back}>Back</Act>
	</Choices>
{:else if mode === "endpoint" || mode === "saving"}
	<form
		class="endpoint"
		onsubmit={(e) => {
			e.preventDefault();
			if (endpointValid) showSudo = true;
		}}
	>
		<p class="help">
			Any endpoint that answers OpenAI-style chat completions with a bearer token: a gateway such as Vercel AI
			Gateway or OpenRouter, a provider's own API, or a local Ollama. The key is stored encrypted on your server,
			and saving it asks for an approval at the server's command line.
		</p>
		<Line bind:value={endpointUrl} type="url" placeholder="https://ai-gateway.vercel.sh/v1/chat/completions" autofocus disabled={mode === "saving"} />
		<Line bind:value={apiKey} type="password" placeholder="API key" disabled={mode === "saving"} />
		<Line bind:value={chatModel} placeholder="What it calls the chat model, if not ours (optional)" disabled={mode === "saving"} />
		<Choices>
			<Act type="submit" variant="primary" disabled={!endpointValid || mode === "saving"}>
				{mode === "saving" ? "Saving…" : "Save the endpoint"}
			</Act>
			<Act variant="plain" onclick={back}>Back</Act>
		</Choices>
	</form>
	<SudoModal
		bind:show={showSudo}
		action="change_byo_key"
		title="Save your endpoint"
		description="Every AI call will route through this endpoint. Confirm at the server's command line."
		onApproved={saveEndpoint}
	/>
{/if}
{#if error}
	<p class="error">{error}</p>
{/if}

<style>
	.endpoint {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		max-width: 34rem;
	}
	.help {
		margin: 0;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}
	.error {
		margin: 0.5rem 0 0;
		font-size: 0.875rem;
		color: var(--color-error, #9a2b2e);
	}
	form {
		margin: 0;
	}
</style>
