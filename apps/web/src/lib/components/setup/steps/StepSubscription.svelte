<!--
	Step 3 — Subscription. The one required step with no way past: the
	interview needs AI, and so does everything the assistant does. Any of
	three ways through settles it — a subscription, signing in to an existing
	account, or your own AI — and all three unfold on this screen.

	WHY ON THIS SCREEN, NOT THE LETTER'S. It was the letter's close
	(2026-09-23), when the letter was the last thing before the app. Setup
	puts the letter first, as a preface, so the subscription's one paragraph
	of reasons came with it: the server keeps the record, the subscription
	pays for the intelligence, and the goal is for all of it to run at home.
	In Adam's voice, still — it is the letter's P.S., moved to where it is
	needed.

	ONE PRIMARY AND TWO QUIET LINKS, not three equal buttons. Three peers made
	paying read as one option of three. The price sits under the button, not
	on it: "$20/mo" in a filled pill reads as "clicking charges me", when the
	click only opens a checkout.

	YOUR OWN AI, WITHOUT A COMMAND LINE. The first key saved during setup,
	from the device that set the server up, needs no approval at the
	server's command line (settings_byo.rs, `first_key_during_setup`): an
	appliance owner has no command line, and the form used to end at one.
	Anyone else, or any later change, still gets the approval, so the save
	tries without it first and asks only when the server says so.

	DONE IS A STATE, NOT A TOAST. Once the server reports AI connected — by
	any of the three paths, or because it already was — the step shows a
	drawn check and the way on. Nothing advances on its own; the check is the
	moment.
-->
<script lang="ts">
	import { onDestroy, onMount } from "svelte";
	import { fly, fade } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import Icon from "$lib/components/Icon.svelte";
	import ThinkingMark from "$lib/components/ThinkingMark.svelte";
	import Input from "$lib/components/Input.svelte";
	import SudoModal from "$lib/components/SudoModal.svelte";
	import { openExternal } from "$lib/tauri/bridge";
	import { setupLinkPoll, setupSubscribeStart, setupLoginStart, setByoKey } from "$lib/api/client";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import StepFrame from "../StepFrame.svelte";
	import { setup, intoLabel } from "../setup.svelte";

	let { onnext }: { onnext: () => void } = $props();

	type Mode = "choose" | "checkout" | "signin" | "sending" | "mailed" | "endpoint" | "saving";
	let mode = $state<Mode>("choose");
	let error = $state<string | null>(null);
	let timer: ReturnType<typeof setInterval> | null = null;

	let email = $state("");
	let endpointUrl = $state("");
	let apiKey = $state("");
	let chatModel = $state("");
	let showSudo = $state(false);
	/** "Chat model" waits behind More options: almost nobody needs it. */
	let moreOptions = $state(false);
	let checkoutUrl = $state<string | null>(null);

	const emailValid = $derived(/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim()));
	const endpointValid = $derived(/^https?:\/\/\S+/.test(endpointUrl.trim()) && apiKey.trim().length > 0);

	// Unknown until the first read lands; an older box (404) has no such
	// state and is treated as set up, the way the store treats it.
	const loaded = $derived(gettingStarted.loaded);
	const done = $derived(gettingStarted.loaded && gettingStarted.aiConnected);
	const via = $derived(gettingStarted.step("connect_ai")?.via ?? null);

	const still =
		typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
	const IN = { y: still ? 0 : 8, duration: still ? 0 : 260, easing: cubicOut, delay: still ? 0 : 60 };
	const OUT = { y: still ? 0 : -6, duration: still ? 0 : 150, easing: cubicOut };

	onMount(() => {
		void gettingStarted.refresh();
	});

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
		mode = mode === next ? "choose" : next;
		if (mode === next) focusField(next === "signin" ? "setup-email" : "setup-endpoint");
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
				if (d.status === "no_account") error = "There's no Virtues account for that email yet. Set up a subscription instead.";
				else if (d.status === "rate_limited") error = "Too many attempts for that email. Try again in an hour.";
				else error = "That didn't go through. Try again.";
			}
		} catch {
			mode = "signin";
			error = UNREACHABLE;
		}
	}
	async function saveEndpoint(sudoRequestId?: string) {
		mode = "saving";
		error = null;
		try {
			await setByoKey({
				...(sudoRequestId ? { sudo_request_id: sudoRequestId } : {}),
				endpoint_url: endpointUrl.trim(),
				api_key: apiKey,
				models: chatModel.trim() ? { chat: chatModel.trim() } : {},
			});
			apiKey = "";
			await gettingStarted.refresh();
		} catch (e) {
			mode = "endpoint";
			// Not the first key during setup: the server wants its approval.
			if (!sudoRequestId && e instanceof Error && e.message === "sudo_not_approved") {
				showSudo = true;
				return;
			}
			error = e instanceof Error ? e.message : "Your server couldn't save that address. Check it, then try again.";
		}
	}

	const UNREACHABLE =
		"Couldn't reach the Virtues billing service. Check your server's internet connection, then try again.";

	onDestroy(stop);
</script>

<!-- ONE CARD (2026-09-25). The step used to be a heading, two paragraphs, a
     rule and a button: a billing form. It is now the thing being chosen, as
     an object: what it costs and what it buys, with one way to take it.
     The other two ways through are quiet links under it. The letter's P.S.
     (why there is a subscription at all) stays, as a note at the foot for
     whoever wants the reason. What the card lists is Billing's own claim,
     "one subscription covers all four" (BillingView). -->
<StepFrame title={done ? (via === "byo" ? "Your server is using your own AI" : "You've set up your subscription") : "Choose how your assistant thinks"}>
	{#if !loaded}
		<div class="card placeholder" aria-hidden="true"></div>
	{:else if done}
		<div class="card settled-card" in:fade={{ duration: still ? 0 : 400 }}>
			<span class="check" aria-hidden="true">
				<svg viewBox="0 0 24 24" width="36" height="36">
					<circle cx="12" cy="12" r="11" />
					<path d="M7 12.4l3.3 3.2L17.2 8.6" />
				</svg>
			</span>
			<p class="settled">
				{via === "byo" ? "Every AI call goes through the address you gave." : "Your assistant can answer now."}
			</p>
			<button class="setup-go wide" onclick={onnext}>
				{intoLabel(setup.upNext("subscription"))}
				<Icon icon="ri:arrow-right-line" width="16" />
			</button>
		</div>
	{:else}
		<div class="card">
			<p class="kind">Virtues subscription</p>
			<p class="price"><span class="amount">$20</span> a month</p>
			<ul class="covers">
				<li><Icon icon="ri:check-line" width="16" /> Your assistant, on the best models there are</li>
				<li><Icon icon="ri:check-line" width="16" /> Web search</li>
				<li><Icon icon="ri:check-line" width="16" /> Place search</li>
				<li><Icon icon="ri:check-line" width="16" /> Bank connections</li>
			</ul>
			{#if mode === "checkout"}
				<div class="waiting" in:fly={IN}>
					<span class="wait-mark" aria-hidden="true"><ThinkingMark depth={4} size={18} /></span>
					<p>
						Finish in the window that opened. This page picks up on its own, and Virtues charges nothing until you
						confirm there.
					</p>
				</div>
				<div class="card-row">
					{#if checkoutUrl}
						<button class="link" onclick={() => checkoutUrl && void openExternal(checkoutUrl)}>Open checkout again</button>
					{/if}
					<button class="setup-past" onclick={back}>Back</button>
				</div>
			{:else}
				<button class="setup-go wide" onclick={subscribe}>
					Set up your subscription
					<Icon icon="ri:arrow-right-line" width="16" />
				</button>
				<p class="fine">You'll confirm in your browser.</p>
			{/if}
		</div>

		<div class="links">
			<button
				class="link"
				class:on={mode === "signin" || mode === "sending" || mode === "mailed"}
				aria-expanded={mode === "signin" || mode === "sending" || mode === "mailed"}
				onclick={() => open("signin")}>I already have an account</button
			>
			<span class="dot" aria-hidden="true">·</span>
			<button
				class="link"
				class:on={mode === "endpoint" || mode === "saving"}
				aria-expanded={mode === "endpoint" || mode === "saving"}
				onclick={() => open("endpoint")}>Use my own AI</button
			>
		</div>

		{#if mode === "signin" || mode === "sending"}
			<form
				class="fold narrow"
				in:fly={IN}
				out:fly={OUT}
				onsubmit={(e) => {
					e.preventDefault();
					void signIn();
				}}
			>
				<Input
					id="setup-email"
					type="email"
					bind:value={email}
					placeholder="you@example.com"
					autocomplete="email"
					disabled={mode === "sending"}
					loading={mode === "sending"}
				/>
				<div class="row">
					<button type="submit" class="setup-go small" disabled={!emailValid || mode === "sending"}>
						{mode === "sending" ? "Sending…" : "Email me a sign-in link"}
					</button>
				</div>
			</form>
		{:else if mode === "mailed"}
			<div class="waiting fold narrow" in:fly={IN}>
				<span class="wait-mark" aria-hidden="true"><ThinkingMark depth={4} size={18} /></span>
				<p>A sign-in link is on its way to {email.trim()}. Open it and this page picks up on its own.</p>
			</div>
		{:else if mode === "endpoint" || mode === "saving"}
			<form
				class="fold"
				in:fly={IN}
				out:fly={OUT}
				onsubmit={(e) => {
					e.preventDefault();
					if (endpointValid) void saveEndpoint();
				}}
			>
				<p class="help">
					Any service that answers OpenAI-style chat completions: a gateway such as OpenRouter, a provider's own
					API, or a local Ollama. Your server stores the key encrypted. Your own AI covers your assistant; web
					search, place search and bank connections still need a subscription.
				</p>
				<Input
					id="setup-endpoint"
					label="Address"
					type="url"
					bind:value={endpointUrl}
					placeholder="https://openrouter.ai/api/v1/chat/completions"
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
				{#if moreOptions || chatModel}
					<div in:fly={IN}>
						<Input
							label="Chat model"
							bind:value={chatModel}
							placeholder="Leave empty for the default"
							helperText="Only if this address uses a different name for its chat model."
							disabled={mode === "saving"}
						/>
					</div>
				{:else}
					<button type="button" class="link more" onclick={() => (moreOptions = true)}>More options</button>
				{/if}
				<div class="row">
					<button type="submit" class="setup-go small" disabled={!endpointValid || mode === "saving"}>
						{mode === "saving" ? "Connecting…" : "Connect my AI"}
					</button>
				</div>
			</form>
		{/if}

		{#if error}
			<p class="err" role="alert">
				<Icon icon="ri:error-warning-line" width="15" />
				{error}
			</p>
		{/if}

		<!-- The letter's P.S., moved here with the question it answers. The
		     claim is checked against the build: every default model runs under
		     zero data retention, enforced per request at the gateway. -->
		<aside class="why">
			<p class="why-head">Why a subscription</p>
			<p>
				Your server keeps the record. For now, your assistant borrows its intelligence from the best models there
				are, under terms that keep nothing you send, and a subscription pays for that.
			</p>
			<p>The goal is for all of it to run in your home. The hardware is close, but not yet cheap enough.</p>
		</aside>
	{/if}
</StepFrame>

<SudoModal
	bind:show={showSudo}
	action="change_byo_key"
	title="Connect your own AI"
	description="Every AI call will go through this address. Confirm at the server's command line."
	onApproved={saveEndpoint}
/>

<style>
	/* The card: the thing being chosen, centered, a little lifted off the
	   stage like the letter's sheet. */
	.card {
		width: 100%;
		max-width: 25rem;
		margin: 0 auto;
		padding: 1.75rem 1.75rem 1.5rem;
		border-radius: 12px;
		background: var(--color-surface-overlay, var(--color-surface));
		outline: 1px solid color-mix(in srgb, var(--color-foreground) 9%, transparent);
		outline-offset: -1px;
		text-align: left;
		animation: setup-rise var(--m-slow) var(--m-ease) both;
		animation-delay: 120ms;
	}
	.card.placeholder {
		height: 22rem;
		opacity: 0;
	}
	.kind {
		margin: 0;
		font-size: 13px;
		letter-spacing: 0.02em;
		color: var(--color-foreground-subtle);
	}
	.price {
		margin: 0.4rem 0 0;
		font-size: 15px;
		color: var(--color-foreground-muted);
	}
	.amount {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 3rem;
		line-height: 1;
		letter-spacing: -0.02em;
		color: var(--color-foreground);
		margin-right: 0.25rem;
	}
	.covers {
		margin: 1.25rem 0 1.5rem;
		padding: 1.1rem 0 0;
		border-top: 1px solid var(--color-border);
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		font-size: 15px;
		color: var(--color-foreground);
	}
	.covers li {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}
	.covers :global(svg) {
		flex: none;
		color: var(--color-success);
	}
	:global(.setup-go.wide) {
		width: 100%;
		justify-content: center;
	}
	.fine {
		margin: 0.7rem 0 0;
		text-align: center;
		font-size: 13px;
		color: var(--color-foreground-subtle);
	}
	.card-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		margin-top: 0.5rem;
	}
	.settled-card {
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: 0.9rem;
	}

	/* The reason, for whoever wants it: the letter's P.S., at the foot, in
	   the letter's serif, quiet. */
	.why {
		max-width: 30rem;
		margin: 3rem auto 0;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.975rem;
		line-height: 1.65;
		color: var(--color-foreground-muted);
		text-align: left;
	}
	.why p {
		margin: 0 0 0.8rem;
	}
	.why-head {
		font-family: var(--font-sans, system-ui);
		font-size: 13px;
		letter-spacing: 0.02em;
		color: var(--color-foreground-subtle);
	}


	:global(.setup-go.small) {
		padding: 0.65rem 1.3rem;
		font-size: 14px;
	}

	.links {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-wrap: wrap;
		gap: 0.6rem;
		margin-top: 1.25rem;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground-muted);
		text-decoration: underline;
		text-decoration-color: color-mix(in srgb, currentColor 35%, transparent);
		text-underline-offset: 3px;
		cursor: pointer;
		transition: color 0.15s ease;
	}
	.link:hover,
	.link.on {
		color: var(--color-foreground);
	}
	.link:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 3px;
		border-radius: 0;
	}
	.dot {
		color: var(--color-foreground-subtle);
	}
	.link.more {
		align-self: flex-start;
		font-size: 13px;
	}

	.fold {
		display: flex;
		flex-direction: column;
		gap: 0.875rem;
		max-width: 30rem;
		margin: 1.25rem auto 0;
		text-align: left;
	}
	.fold.narrow {
		max-width: 22rem;
	}
	.row {
		display: flex;
	}
	.help {
		margin: 0;
		font-size: 14px;
		line-height: 1.55;
		color: var(--color-foreground-muted);
	}

	.waiting {
		display: flex;
		align-items: baseline;
		gap: 0.65rem;
		margin: 0 0 0.75rem;
		font-size: 14px;
		line-height: 1.55;
		color: var(--color-foreground-muted);
	}
	.waiting.fold {
		margin-top: 1.25rem;
	}
	/* Waiting on the browser or the inbox: the ∴ at depth 4, which means
	   "gone out to something" everywhere else in the app too — not a
	   generic pulse. */
	.wait-mark {
		flex: none;
		display: inline-grid;
		color: var(--color-primary);
		transform: translateY(3px);
	}

	.err {
		margin-top: 1rem;
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 13px;
		color: var(--color-error);
	}

	/* THE CHECK. The ring draws, then the tick — the only celebration on the
	   page, and it is a mark rather than a message. Green is the theme's own
	   success token, so it holds on every theme. */
	.settled {
		display: flex;
		align-items: center;
		gap: 0.7rem;
		margin: 0 0 1.5rem;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.25rem;
		color: var(--color-foreground);
	}
	.check svg {
		display: block;
		fill: none;
		stroke: var(--color-success);
		stroke-width: 1.6;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.check circle {
		stroke-dasharray: 70;
		stroke-dashoffset: 70;
		animation: draw 700ms cubic-bezier(0.2, 0.7, 0.2, 1) forwards;
	}
	.check path {
		stroke-width: 2;
		stroke-dasharray: 16;
		stroke-dashoffset: 16;
		animation: draw 380ms cubic-bezier(0.2, 0.7, 0.2, 1) 520ms forwards;
	}
	@keyframes draw {
		to {
			stroke-dashoffset: 0;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.check circle,
		.check path {
			animation: none;
			stroke-dashoffset: 0;
		}
	}
</style>
