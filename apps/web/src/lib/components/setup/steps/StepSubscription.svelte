<!--
	Subscription, the last step before Arrival. The one required step with
	no way past: everything the assistant does needs AI. Any of three ways
	through settles it (a subscription, signing in to an existing account,
	or your own AI) and all three unfold on this screen. Its one paragraph
	of reasons, at the foot, is the letter's P.S., in Adam's voice, moved to
	where it is needed.

	YOUR OWN AI, WITHOUT A COMMAND LINE. Saving the key carries the app's
	owner proof (api/owner_proof.rs), which the app signs with this device's
	key without an extra step, so setup never sends anyone to a command line
	for it.

	TWO CHOICES, SIDE BY SIDE (2026-11-16, the revised Setup the owner
	approved). The subscription and your own AI are the two ways an
	assistant gets to think, so they are the two things on the page, as
	choices; the subscription is chosen when the page opens, and the one way
	forward follows the choice. Signing in to an account that already pays is
	a link under them, because it is not a third kind of AI.

	DONE IS A STATE, NOT A TOAST. Once the server reports AI connected — by
	any of the three paths, or because it already was — the step shows a
	drawn check and the way on. Nothing advances on its own; the check is the
	moment.
-->
<script lang="ts">
	import { onDestroy, onMount } from "svelte";
	import { reducedMotion } from "$lib/stores/motion.svelte";
	import { fly, fade } from "$lib/utils/transitions";
	import { cubicOut } from "svelte/easing";
	import Icon from "$lib/components/Icon.svelte";
	import ThinkingMark from "$lib/components/ThinkingMark.svelte";
	import Input from "$lib/components/Input.svelte";
	import { openExternal } from "$lib/tauri/bridge";
	import { setupLinkPoll, setupLoginStart, setByoKey } from "$lib/api/client";
	import { startCheckout } from "$lib/api/checkout";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import StepFrame from "../StepFrame.svelte";
	import { setup, intoLabel } from "../setup.svelte";

	let {
		onnext,
		nextLabel,
	}: {
		onnext: () => void;
		/** What the way on says, when the host knows better than the stage's
		 *  order (in the app, the next step there, or the list). */
		nextLabel?: string;
	} = $props();

	type Mode = "choose" | "checkout" | "signin" | "sending" | "mailed" | "endpoint" | "saving";
	let mode = $state<Mode>("choose");
	/** Which way the assistant thinks, as chosen on the page. */
	let pick = $state<"sub" | "byo">("sub");
	function choose(next: "sub" | "byo") {
		if (mode === "checkout" || mode === "saving") return;
		pick = next;
		error = null;
		mode = next === "byo" ? "endpoint" : "choose";
		if (next === "byo") focusField("setup-endpoint");
	}

	/** A radio group's keys: the arrows move the choice and its focus, and
	 *  only the chosen tile is a Tab stop. */
	function onChoiceKey(e: KeyboardEvent) {
		const fwd = e.key === "ArrowRight" || e.key === "ArrowDown";
		const back = e.key === "ArrowLeft" || e.key === "ArrowUp";
		if (!fwd && !back) return;
		e.preventDefault();
		const next = pick === "sub" ? "byo" : "sub";
		if (mode === "checkout" || mode === "saving") return;
		pick = next;
		error = null;
		mode = next === "byo" ? "endpoint" : "choose";
		document.querySelector<HTMLElement>(`.choice[data-pick="${next}"]`)?.focus();
	}
	let error = $state<string | null>(null);
	let timer: ReturnType<typeof setInterval> | null = null;

	let email = $state("");
	let endpointUrl = $state("");
	let apiKey = $state("");
	let chatModel = $state("");
	/** "Chat model" waits behind More options: almost nobody needs it. */
	let moreOptions = $state(false);
	let checkoutUrl = $state<string | null>(null);
	/** Why the email field is open: to sign in, or as the first half of
	 *  subscribing (a checkout needs an account to belong to). */
	let intent = $state<"signin" | "subscribe">("signin");

	const emailValid = $derived(/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim()));
	const endpointValid = $derived(/^https?:\/\/\S+/.test(endpointUrl.trim()) && apiKey.trim().length > 0);

	// Unknown until the first read lands; an older box (404) has no such
	// state and is treated as set up, the way the store treats it.
	const loaded = $derived(gettingStarted.loaded);
	const done = $derived(gettingStarted.loaded && gettingStarted.aiConnected);
	const via = $derived(gettingStarted.step("connect_ai")?.via ?? null);
	/** Signed in already (before pairing, or earlier), with no subscription on
	 *  the account: signing in again would change nothing. */
	const signedIn = $derived(via === "linked");
	/** Named on Names, earlier in Setup. */
	const name = $derived(setup.assistantName);

	const still =
		reducedMotion();
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
		mode = mode === next ? "choose" : next;
		if (mode === next) focusField(next === "signin" ? "setup-email" : "setup-endpoint");
	}

	/** Wait for the subscription to land. Ten minutes of checks: long
	 *  enough for a card and a 3-D Secure prompt. */
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
	 *  first; `afterSignIn` is the second try, once the link is ready. */
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
				focusField("setup-email");
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
		error = null;
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
<!-- WHY, ON THE SCREEN (2026-09-28): this is the one step with no way past,
     and the page never said so. It is said here only (not on Account). -->
<StepFrame
	centered={false}
	title={done ? (via === "byo" ? "Your server is using your own AI" : "You've set up your subscription") : `Choose how ${name} thinks`}
	subtitle={done
		? undefined
		: signedIn
			? `You're signed in, and your account has no subscription yet. ${name} can't answer anything until it has AI to think with.`
			: `${name} can't answer anything until it has AI to think with. A Virtues subscription gives it the best models there are, or you can connect your own.`}
>
	{#if !loaded}
		<div class="placeholder" aria-hidden="true"></div>
	{:else if done}
		<div class="settled-card" in:fade={{ duration: still ? 0 : 400 }}>
			<span class="check" aria-hidden="true">
				<svg viewBox="0 0 24 24" width="36" height="36">
					<circle cx="12" cy="12" r="11" />
					<path d="M7 12.4l3.3 3.2L17.2 8.6" />
				</svg>
			</span>
			<p class="settled">
				{via === "byo" ? "Every AI call goes through the address you gave." : `${name} can answer now.`}
			</p>
			<button class="setup-go wide" onclick={onnext}>
				{nextLabel ?? intoLabel(setup.upNext("subscription"))}
				<Icon icon="mynaui:arrow-right" width="16" />
			</button>
		</div>
	{:else}
		<div class="choices" role="radiogroup" aria-label="How {name} thinks">
			<button
				type="button"
				class="choice"
				role="radio"
				data-pick="sub"
				aria-checked={pick === "sub"}
				tabindex={pick === "sub" ? 0 : -1}
				onclick={() => choose("sub")}
				onkeydown={onChoiceKey}
			>
				<span class="choice-head">
					<span class="choice-name">A Virtues subscription</span>
					<span class="radio" aria-hidden="true"></span>
				</span>
				<span class="choice-line">Includes $20 of AI each month, plus bank data, places, maps and remote access. Extra AI is sold at cost.</span>
				<span class="price"><span class="amount">$29</span> a month</span>
			</button>
			<button
				type="button"
				class="choice"
				role="radio"
				data-pick="byo"
				aria-checked={pick === "byo"}
				tabindex={pick === "byo" ? 0 : -1}
				onclick={() => choose("byo")}
				onkeydown={onChoiceKey}
			>
				<span class="choice-head">
					<span class="choice-name">Your own AI</span>
					<span class="radio" aria-hidden="true"></span>
				</span>
				<span class="choice-line">An API key from a provider, or a model you run yourself. It covers your assistant.</span>
				<span class="price quiet">You pay your provider</span>
			</button>
		</div>

		{#if pick === "sub"}
			{#if mode === "checkout"}
				<div class="waiting fold narrow" in:fly={IN}>
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
				<div class="go">
					<button class="setup-go" onclick={() => subscribe()}>
						Continue to checkout
						<Icon icon="mynaui:arrow-right" width="16" />
					</button>
					<p class="fine">{signedIn ? "You'll confirm in your browser." : "We'll confirm your email first, then you confirm in your browser."}</p>
				</div>
			{/if}
		{/if}

		{#if !signedIn && pick === "sub" && mode !== "checkout" && intent === "signin"}
			<div class="links">
				<button
					class="link"
					class:on={mode === "signin" || mode === "sending" || mode === "mailed"}
					aria-expanded={mode === "signin" || mode === "sending" || mode === "mailed"}
					onclick={() => open("signin")}>I already have an account</button
				>
			</div>
		{/if}

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
				{#if intent === "subscribe"}
					<p class="help">First, your email. We'll send a link, and checkout opens once you've opened it.</p>
				{/if}
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
						{mode === "sending" ? "Sending…" : intent === "subscribe" ? "Email me a link" : "Email me a sign-in link"}
					</button>
				</div>
			</form>
		{:else if mode === "mailed"}
			<div class="waiting fold narrow" in:fly={IN}>
				<span class="wait-mark" aria-hidden="true"><ThinkingMark depth={4} size={18} /></span>
				<p>
					{intent === "subscribe"
						? `A link is on its way to ${email.trim()}. Open it and choose Attach box, then continue to checkout.`
						: `A sign-in link is on its way to ${email.trim()}. Open it and choose Attach box, and this page picks up on its own.`}
				</p>
			</div>
			<div class="card-row">
				<button class="setup-past" onclick={back}>Back</button>
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
					Any service that answers OpenAI-style chat completions: a gateway such as OpenRouter, a provider's own API,
					or a local Ollama. Your server stores the key encrypted. Web search, place search and bank connections
					still need a subscription.
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
				<Icon icon="mynaui:danger-circle" width="15" />
				{error}
			</p>
		{/if}

		<!-- The letter's P.S., moved here with the question it answers. The
		     claim is checked against the build: every default model runs under
		     zero data retention, enforced per request at the gateway. -->
		{#if pick === "sub"}
			<aside class="why">
				<p class="why-head">Why a subscription</p>
				<p>
					Your server keeps the record of your life. Keeping it well takes a librarian: models that read each day,
					write it up, and find what you ask for. Every one runs under zero data retention (ZDR): nothing you send
					is stored or trained on.
				</p>
				<p>The goal is for all of it to run in your home. The hardware is close, but not yet cheap enough.</p>
			</aside>
		{/if}
	{/if}
</StepFrame>

<style>
	/* The two ways an assistant thinks, side by side: each a hairline tile
	   on the sheet, the chosen one drawn in the primary ink. */
	.choices {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 12px;
		text-align: left;
		animation: setup-rise var(--m-slow) var(--m-ease) both;
		animation-delay: 120ms;
	}
	@media (max-width: 560px) {
		.choices {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	.choice {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 18px 18px 16px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		background: transparent;
		font: inherit;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
		transition:
			border-color var(--m-quick) ease,
			background-color var(--m-quick) ease,
			transform var(--m-base) var(--m-spring);
	}
	.choice:hover {
		background: color-mix(in srgb, var(--color-foreground) 3%, transparent);
	}
	.choice:active {
		transform: scale(0.985);
		transition-duration: 90ms;
	}
	.choice[aria-checked="true"] {
		border-color: var(--color-primary);
		outline: 3px solid color-mix(in srgb, var(--color-primary) 10%, transparent);
		background: color-mix(in srgb, var(--color-primary) 3%, transparent);
	}
	.choice-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.choice-name {
		font-size: 16px;
		font-weight: 500;
	}
	.radio {
		flex: none;
		width: 18px;
		height: 18px;
		box-sizing: border-box;
		border-radius: 50%;
		border: 1.5px solid var(--color-border);
		transition:
			border-width var(--m-base) var(--m-spring),
			border-color var(--m-quick) ease;
	}
	.choice[aria-checked="true"] .radio {
		border: 5px solid var(--color-primary);
	}
	.choice-line {
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.price {
		margin-top: auto;
		padding-top: 6px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.price.quiet {
		line-height: 30px;
	}
	.amount {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 26px;
		line-height: 1;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		margin-right: 0.2rem;
	}
	.go {
		margin-top: 1.5rem;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
	}
	.placeholder {
		height: 12rem;
	}
	:global(.setup-go.wide) {
		width: 100%;
		justify-content: center;
	}
	.fine {
		margin: 0.7rem 0 0;
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
		max-width: 25rem;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.9rem;
	}

	/* The reason, for whoever wants it: the letter's P.S., at the foot, in
	   the letter's serif, quiet. */
	.why {
		max-width: 32rem;
		margin: 2.5rem 0 0;
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
		justify-content: flex-start;
		flex-wrap: wrap;
		gap: 0.6rem;
		margin-top: 1.25rem;
	}
	.link {
		position: relative;
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
	/* A 44-point hit area around a line of text. */
	.link::after {
		content: "";
		position: absolute;
		inset: -12px -4px;
	}
	.link:hover,
	.link.on {
		color: var(--color-foreground);
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
		margin: 1.25rem 0 0;
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
