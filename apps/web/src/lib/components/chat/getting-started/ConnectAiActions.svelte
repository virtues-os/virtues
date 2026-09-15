<!--
	Connect AI's controls: three doors in one row. Sign in unfolds in place
	into one field and a send; an endpoint of your own unfolds into three
	fields and the approval at the server's command line. Keys are typed here
	and nowhere else in the room, and never reach the transcript.

	The fields are the app's own `Input` — the same control Settings and the
	profile use. They were a hairline `Line` invented for this room alone,
	which made the one screen a new arrival sees the one screen that looks
	like nothing else in the product.
-->
<script lang="ts">
	import { onDestroy } from "svelte";
	import { fly } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import SudoModal from "$lib/components/SudoModal.svelte";
	import Input from "$lib/components/Input.svelte";
	import { openExternal } from "$lib/tauri/bridge";
	import { setupLinkPoll, setupSubscribeStart, setupLoginStart, setByoKey } from "$lib/api/client";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import Act from "./ui/Act.svelte";
	import Choices from "./ui/Choices.svelte";
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

	// One door swaps for another in the same spot, so the panes cross rather
	// than cut: the outgoing one lifts and fades where it stands (the wrapper
	// stacks both in one grid cell, so nothing collapses mid-swap) and the
	// incoming one settles in from just below. Anyone who has asked not to be
	// moved gets the swap with no travel and no time.
	const still =
		typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
	const IN = { y: still ? 0 : 8, duration: still ? 0 : 260, easing: cubicOut, delay: still ? 0 : 60 };
	const OUT = { y: still ? 0 : -6, duration: still ? 0 : 150, easing: cubicOut };

	/** The field a newly opened pane should be typing into. Retried for a
	    few frames rather than grabbed once: the pane is mounting behind a
	    transition, and a single rAF can land before the input exists (or not
	    run at all when the window is not compositing). */
	function focusField(id: string) {
		let tries = 0;
		const grab = () => {
			const el = document.getElementById(id);
			if (el) return el.focus();
			if (tries++ < 12) setTimeout(grab, 16);
		};
		setTimeout(grab, 0);
	}

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
	function open(next: "signin" | "endpoint") {
		error = null;
		mode = next;
		focusField(next === "signin" ? "gs-email" : "gs-endpoint");
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

	const UNREACHABLE =
		"Couldn't reach the Virtues billing service. Check your server's internet connection and try again.";

	onDestroy(stop);
</script>

<div class="stack">
	{#if mode === "choose"}
		<div class="pane" in:fly={IN} out:fly={OUT}>
			<Choices>
				<Act variant="primary" onclick={subscribe}>Subscribe · $20/mo</Act>
				<Act onclick={() => open("signin")}>Sign in</Act>
				<Act onclick={() => open("endpoint")}>Own models</Act>
			</Choices>
		</div>
	{:else if mode === "signin" || mode === "sending"}
		<div class="pane" in:fly={IN} out:fly={OUT}>
			<form
				class="form narrow"
				onsubmit={(e) => {
					e.preventDefault();
					void signIn();
				}}
			>
				<Input
					id="gs-email"
					type="email"
					bind:value={email}
					placeholder="you@example.com"
					autocomplete="email"
					disabled={mode === "sending"}
					loading={mode === "sending"}
				/>
				<Choices>
					<Act type="submit" variant="primary" disabled={!emailValid || mode === "sending"}>
						{mode === "sending" ? "Sending…" : "Send a link"}
					</Act>
					<Act variant="plain" onclick={back}>Back</Act>
				</Choices>
			</form>
		</div>
	{:else if mode === "mailed"}
		<div class="pane" in:fly={IN} out:fly={OUT}>
			<Choices>
				<Waiting>A sign-in link is in your email. This picks up on its own once you have used it.</Waiting>
				<Act variant="plain" onclick={back}>Back</Act>
			</Choices>
		</div>
	{:else if mode === "checkout"}
		<div class="pane" in:fly={IN} out:fly={OUT}>
			<Choices>
				<Waiting>Finish in the window that opened. This picks up on its own after checkout.</Waiting>
				<Act variant="plain" onclick={back}>Back</Act>
			</Choices>
		</div>
	{:else if mode === "endpoint" || mode === "saving"}
		<div class="pane" in:fly={IN} out:fly={OUT}>
			<form
				class="form"
				onsubmit={(e) => {
					e.preventDefault();
					if (endpointValid) showSudo = true;
				}}
			>
				<p class="help">
					Any endpoint that answers OpenAI-style chat completions with a bearer token: a gateway such as
					Vercel AI Gateway or OpenRouter, a provider's own API, or a local Ollama. The key is stored
					encrypted on your server, and saving it asks for an approval at the server's command line.
				</p>
				<Input
					id="gs-endpoint"
					label="Endpoint"
					type="url"
					bind:value={endpointUrl}
					placeholder="https://ai-gateway.vercel.sh/v1/chat/completions"
					disabled={mode === "saving"}
				/>
				<Input
					label="API key"
					type="password"
					bind:value={apiKey}
					placeholder="sk-…"
					autocomplete="off"
					disabled={mode === "saving"}
				/>
				<Input
					label="Chat model"
					bind:value={chatModel}
					placeholder="Leave empty to keep ours"
					helperText="Only if this endpoint names the model differently."
					disabled={mode === "saving"}
				/>
				<Choices>
					<Act type="submit" variant="primary" disabled={!endpointValid || mode === "saving"}>
						{mode === "saving" ? "Saving…" : "Save the endpoint"}
					</Act>
					<Act variant="plain" onclick={back}>Back</Act>
				</Choices>
			</form>
		</div>
	{/if}
</div>
<SudoModal
	bind:show={showSudo}
	action="change_byo_key"
	title="Save your endpoint"
	description="Every AI call will route through this endpoint. Confirm at the server's command line."
	onApproved={saveEndpoint}
/>
{#if error}
	<p class="error">{error}</p>
{/if}

<style>
	/* Both panes occupy one cell, so the outgoing one can still be fading
	   where it stood while the incoming one arrives — a plain `{#if}` would
	   empty the row first and drop everything under it by the height of a
	   button. */
	.stack {
		display: grid;
	}
	.stack > .pane {
		grid-area: 1 / 1;
		align-self: start;
		min-width: 0;
	}
	.form {
		display: flex;
		flex-direction: column;
		gap: 0.875rem;
		max-width: 34rem;
		margin: 0.875rem 0 0;
	}
	/* One short answer deserves one short field. */
	.form.narrow {
		max-width: 22rem;
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
</style>
