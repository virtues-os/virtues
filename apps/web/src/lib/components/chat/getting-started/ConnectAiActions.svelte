<!--
	Connect AI's controls: three doors in one row. Sign in unfolds in place
	into one field and a send; an endpoint of your own unfolds into three
	fields, saved with the app's owner proof (`apiSendProven`). Keys are typed here
	and nowhere else in the room, and never reach the transcript.

	The fields are the app's own `Input` — the same control Settings and the
	profile use. They were a hairline `Line` invented for this room alone,
	which made the one screen a new arrival sees the one screen that looks
	like nothing else in the product.
-->
<script lang="ts">
	import { onDestroy } from "svelte";
	import { reducedMotion } from "$lib/stores/motion.svelte";
	import { fly } from "$lib/utils/transitions";
	import { cubicOut } from "svelte/easing";
	import Input from "$lib/components/Input.svelte";
	import { openExternal } from "$lib/tauri/bridge";
	import { setupLinkPoll, setupLoginStart, setByoKey } from "$lib/api/client";
	import { startCheckout } from "$lib/api/checkout";
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
	let checkoutUrl = $state<string | null>(null);
	/** Why the email field is open: to sign in, or as the first half of
	    subscribing (a checkout needs an account to belong to). */
	let intent = $state<"signin" | "subscribe">("signin");

	const emailValid = $derived(/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim()));
	const endpointValid = $derived(/^https?:\/\/\S+/.test(endpointUrl.trim()) && apiKey.trim().length > 0);

	// One door swaps for another in the same spot, so the panes cross rather
	// than cut: the outgoing one lifts and fades where it stands (the wrapper
	// stacks both in one grid cell, so nothing collapses mid-swap) and the
	// incoming one settles in from just below. Anyone who has asked not to be
	// moved gets the swap with no travel and no time.
	const still =
		reducedMotion();
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

	/** Bumped by every stop(), so a tick still awaiting the server when its
	    poll was stopped or replaced cannot act on the new state. */
	let pollGen = 0;
	function stop() {
		if (timer) clearInterval(timer);
		timer = null;
		pollGen += 1;
	}
	function poll() {
		stop();
		const gen = pollGen;
		timer = setInterval(async () => {
			try {
				const d = await setupLinkPoll<{ status: string }>();
				if (gen !== pollGen) return;
				if (d.status === "ready") {
					stop();
					// Paid accounts resolve as already subscribed; a new or
					// unpaid account goes on to checkout.
					await subscribe(true);
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
		intent = "signin";
		mode = "choose";
	}
	function open(next: "signin" | "endpoint") {
		error = null;
		intent = "signin";
		mode = next;
		focusField(next === "signin" ? "gs-email" : "gs-endpoint");
	}

	/** Wait for the subscription to land. Ten minutes of checks: long
	    enough for a card and a 3-D Secure prompt. */
	function pollPaid() {
		stop();
		const gen = pollGen;
		let ticks = 0;
		timer = setInterval(async () => {
			ticks += 1;
			await gettingStarted.refresh();
			if (gen !== pollGen) return;
			if (gettingStarted.aiConnected) stop();
			else if (ticks >= 120) {
				stop();
				mode = "choose";
				intent = "signin";
				error = "We stopped waiting for checkout. If you paid, this updates within a minute. Otherwise, try again.";
			}
		}, 5000);
	}
	/** Open checkout for the linked account. An unlinked server signs in
	    first; `afterSignIn` is the second try, once the link is ready. */
	async function subscribe(afterSignIn = false) {
		error = null;
		try {
			const r = await startCheckout();
			if (r.kind === "opened") {
				checkoutUrl = r.url;
				mode = "checkout";
				// After an emailed sign-in the browser already shows a checkout
				// link on the attach page, so this one waits for a click.
				if (!afterSignIn) void openExternal(r.url);
				pollPaid();
			} else if (r.kind === "needs_signin" && !afterSignIn) {
				intent = "subscribe";
				mode = "signin";
				focusField("gs-email");
			} else if (r.kind === "already_subscribed") {
				await gettingStarted.refresh();
			} else {
				mode = "choose";
				intent = "signin";
				error = r.kind === "error" ? r.message : "Couldn't open checkout. Try again.";
			}
		} catch {
			mode = "choose";
			intent = "signin";
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
				if (d.status === "rate_limited") error = "Too many attempts for that email. Try again in an hour.";
				else error = "That didn't go through. Try again.";
			}
		} catch {
			mode = "signin";
			error = UNREACHABLE;
		}
	}
	async function saveEndpoint() {
		mode = "saving";
		try {
			await setByoKey({
				endpoint_url: endpointUrl.trim(),
				api_key: apiKey,
				// `chat` is the Standard slot's old name. Every box reads it
				// (a renamed one maps it to `standard`); a box older than the
				// rename reads nothing else.
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
				{#snippet foot()}
					<!-- The price belongs in view but not on the button: "$29/mo"
					     set into a filled dark pill reads as "clicking charges
					     me", when the click only opens a checkout. Naming the
					     confirmation step is what actually settles that. -->
					$29 a month, with $20 of AI included. You'll confirm in your browser.
				{/snippet}
				<Act variant="primary" onclick={() => subscribe()}>Subscribe</Act>
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
				{#if intent === "subscribe"}
					<p class="help">First, your email. We'll send a link, and checkout opens once you've opened it.</p>
				{/if}
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
				<Waiting>
					{intent === "subscribe"
						? "A link is on its way to your email. Open it and choose Attach box, then continue to checkout."
						: "A sign-in link is on its way to your email. Open it and choose Attach box, and this picks up on its own. You can leave this where it is."}
				</Waiting>
				<Act variant="plain" onclick={back}>Back</Act>
			</Choices>
		</div>
	{:else if mode === "checkout"}
		<div class="pane" in:fly={IN} out:fly={OUT}>
			<Choices>
				<Waiting>Finish checkout in your browser. This picks up on its own once you have, and we charge nothing until you confirm there.</Waiting>
				{#if checkoutUrl}
					<!-- A checkout opened from a timer (after the emailed sign-in)
					     has no click behind it, and a browser may block it. -->
					<Act onclick={() => checkoutUrl && void openExternal(checkoutUrl)}>Open checkout</Act>
				{/if}
				<Act variant="plain" onclick={back}>Back</Act>
			</Choices>
		</div>
	{:else if mode === "endpoint" || mode === "saving"}
		<div class="pane" in:fly={IN} out:fly={OUT}>
			<form
				class="form"
				onsubmit={(e) => {
					e.preventDefault();
					if (endpointValid) void saveEndpoint();
				}}
			>
				<p class="help">
					Any endpoint that answers OpenAI-style chat completions with a bearer token: a gateway such as
					Vercel AI Gateway or OpenRouter, a provider's own API, or a local Ollama. The key is stored
					encrypted on your server.
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
