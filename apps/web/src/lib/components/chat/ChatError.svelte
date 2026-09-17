<script lang="ts">
	// What a failed turn says for itself. One serif line names what happened,
	// one sans sentence says what to do, one quiet verb does it. No fill, no
	// stripe, no icon: a hairline card on the page, the same object as any
	// other thing here that can be acted on (agents/build/design-grammar.md).
	// The provider's own text is shown only when it adds something we could
	// not say better, and never more than a line of it.
	//
	// The verb is `TextAction` whether it navigates or acts: this block had the
	// same `.notice-action` class on two `<a>`s and two `<button>`s, which is
	// the pair `TextAction`'s `href` exists to keep as one component.
	import TextAction from "$lib/components/TextAction.svelte";

	interface Props {
		error: { message?: string } | null;
		onRetry: () => void;
		/** Display name of the Recommended model to fall back to. When set (with
		 *  `onSwitchAndRetry`), a model-side error offers a switch instead of a
		 *  retry that would just re-hit the same broken model. */
		recommendedName?: string;
		onSwitchAndRetry?: () => void;
	}

	let { error, onRetry, recommendedName, onSwitchAndRetry }: Props = $props();

	const raw = $derived(error?.message ?? "");
	const has = (re: RegExp) => re.test(raw);

	// Core embeds the upstream HTTP status as "(status NNN)" in LLM error
	// messages (StreamError::LlmError); the box's own rejections carry
	// "HTTP NNN". Classify by the real status, not by loose text — a 400
	// "invalid argument" is not a rate limit.
	const status = $derived.by(() => {
		const m = raw.match(/status (\d{3})|HTTP (\d{3})/);
		return m ? Number(m[1] ?? m[2]) : undefined;
	});

	type Kind =
		| "wallet_empty"
		| "card_declined"
		| "monthly_cap"
		| "topup_disabled"
		| "subscription"
		| "reconnect"
		| "rate_limit"
		| "too_large"
		| "interrupted"
		| "output_limit"
		| "model_error"
		| "generic";

	// Billing states first and explicitly, so a 402 "wallet empty" is never
	// mislabeled as a rate limit (that mislabel once cost hours).
	const kind = $derived.by((): Kind => {
		if (has(/wallet_empty|insufficient_budget/i)) return "wallet_empty";
		if (has(/card_declined/i)) return "card_declined";
		if (has(/monthly_cap_reached/i)) return "monthly_cap";
		if (has(/topup_disabled/i)) return "topup_disabled";
		if (has(/wallet_expired|subscription_inactive/i)) return "subscription";
		if (has(/unknown_key|missing_key|malformed_key/i) || status === 401) return "reconnect";
		if (status === 429 || (status === undefined && /rate limit|too many requests|\b429\b/i.test(raw)))
			return "rate_limit";
		// The box refused the body outright (VIR-291): a long paste, a stack
		// of images. Retrying the same message cannot help.
		if (status === 413 || has(/length limit exceeded|payload too large/i)) return "too_large";
		// The stream stopped before the model finished (VIR-334). What arrived
		// is on the page above this card.
		if (has(/stream interrupted|dropped mid-reply|before the model finished|before it was finished/i))
			return "interrupted";
		if (has(/output limit|output cap/i)) return "output_limit";
		// A 4xx the model itself raised — unsupported tools or modality,
		// context overflow, a bad request. Retrying the SAME model just
		// re-fails, so this offers the Recommended model instead.
		if (status === 400 || status === 404 || status === 422) return "model_error";
		return "generic";
	});

	const isBilling = $derived(
		["wallet_empty", "card_declined", "monthly_cap", "topup_disabled", "subscription"].includes(kind)
	);
	const canSwitch = $derived(kind === "model_error" && !!onSwitchAndRetry && !!recommendedName);

	// The provider's sentence, with our wrappers stripped and its JSON opened.
	// Shown for the kinds where we have nothing more exact to say; capped so a
	// stack trace or a 240-model allowlist never lands on the page.
	const detail = $derived.by(() => {
		let msg = raw;
		msg = msg.replace(/^LLM error \(status \d{3}\):\s*/i, "");
		msg = msg.replace(/^Stream interrupted:\s*/i, "");
		try {
			const j = JSON.parse(msg);
			const inner = j?.error?.message ?? j?.message;
			if (typeof inner === "string" && inner) msg = inner;
		} catch {
			// not JSON — leave as-is
		}
		msg = msg.replace(/\s+/g, " ").trim();
		if (!msg) return "";
		msg = msg.charAt(0).toUpperCase() + msg.slice(1);
		if (!/[.!?]$/.test(msg)) msg += ".";
		return msg.length > 160 ? msg.slice(0, 157).trimEnd() + "…" : msg;
	});

	// Titles, not sentences (no trailing period); the line under each is one
	// sentence that says what to do. `sentence: null` means show the
	// provider's own words instead.
	const COPY: Record<Kind, { title: string; sentence: string | null }> = {
		wallet_empty: {
			title: "Out of credits",
			sentence: "Add credits to keep going. Your monthly allotment refreshes on renewal.",
		},
		card_declined: {
			title: "Card declined",
			sentence: "The auto top-up could not charge your card. Update the payment method in Billing.",
		},
		monthly_cap: {
			title: "Monthly cap reached",
			sentence: "Raise it in Billing, or wait for the reset.",
		},
		topup_disabled: {
			title: "Auto top-up is off",
			sentence: "Add credits in Billing, or bring your own AI key.",
		},
		subscription: {
			title: "Subscription inactive",
			sentence: "Reconnect or update billing to continue.",
		},
		reconnect: {
			title: "This box is not recognized by billing",
			sentence: "Reconnect your subscription to continue.",
		},
		rate_limit: {
			title: "The model is busy",
			sentence: "The provider is rate-limiting for a moment. Try again shortly.",
		},
		too_large: {
			title: "That message is too large to send",
			sentence: "Shorten it, or attach the long part as a file.",
		},
		interrupted: {
			title: "The reply was cut off",
			sentence: "The connection to the model dropped before it finished. What arrived is above.",
		},
		output_limit: {
			title: "The reply ran out of room",
			sentence: "The model reached its output limit. What it wrote is above; ask it to continue.",
		},
		model_error: {
			title: "This model could not take that",
			sentence: null,
		},
		generic: {
			title: "The reply did not come through",
			sentence: null,
		},
	};

	const title = $derived(COPY[kind].title);
	const sentence = $derived(COPY[kind].sentence ?? detail);
</script>

{#if error}
	<div class="flex justify-start">
		<div class="reply-notice" role="alert">
			<p class="notice-title">{title}</p>
			{#if sentence}
				<p class="notice-sentence">{sentence}</p>
			{/if}
			<div class="notice-actions">
				{#if isBilling}
					<TextAction href="/billing">
						{kind === "wallet_empty" || kind === "topup_disabled" ? "Add credits" : "Manage billing"}
					</TextAction>
				{:else if kind === "reconnect"}
					<!-- The account gate lives on the getting-started page now,
					     which shows itself at the app root while unsatisfied. -->
					<TextAction href="/">Reconnect</TextAction>
				{:else if canSwitch}
					<TextAction onclick={onSwitchAndRetry}>
						Switch to {recommendedName} and try again
					</TextAction>
				{:else if kind !== "too_large"}
					<TextAction onclick={onRetry}>Try again</TextAction>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	.reply-notice {
		max-width: 36rem;
		margin-top: 8px;
		padding: 16px 20px;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
	}

	.notice-title {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 18px;
		font-weight: 400;
		line-height: 1.3;
		color: var(--color-foreground);
	}

	.notice-sentence {
		margin: 8px 0 0;
		font-family: var(--font-sans);
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.notice-actions {
		margin-top: 12px;
	}

	.notice-actions:empty {
		display: none;
	}
</style>
