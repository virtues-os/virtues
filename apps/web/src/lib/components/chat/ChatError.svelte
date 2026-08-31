<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	interface Props {
		error: { message?: string } | null;
		onRetry: () => void;
		/** Display name of the Recommended model to fall back to. When set (with
		 *  `onSwitchAndRetry`), a model-side error offers a one-click switch
		 *  instead of a plain Retry that would just re-hit the same broken model. */
		recommendedName?: string;
		onSwitchAndRetry?: () => void;
	}

	let { error, onRetry, recommendedName, onSwitchAndRetry }: Props = $props();

	// Core embeds the upstream HTTP status as "(status NNN)" in LLM error messages
	// (see StreamError::LlmError). Classify by the real status — not loose text-matching,
	// which mislabels things like a 400 "invalid argument" as a rate limit.
	const status = $derived.by(() => {
		const m = error?.message?.match(/status (\d{3})/);
		return m ? Number(m[1]) : undefined;
	});

	// The box embeds the virtues-api / atlas error code in the message body.
	// Classify billing states explicitly so a 402 "wallet empty" is never
	// mislabeled "Rate Limit Reached" (that mislabel sent us chasing the wrong
	// cause for hours).
	const raw = $derived(error?.message ?? "");
	const has = (re: RegExp) => re.test(raw);

	// Distinct billing kinds, in priority order.
	const kind = $derived.by((): string => {
		if (has(/wallet_empty|insufficient_budget/i)) return "wallet_empty";
		if (has(/card_declined/i)) return "card_declined";
		if (has(/monthly_cap_reached/i)) return "monthly_cap";
		if (has(/topup_disabled/i)) return "topup_disabled";
		if (has(/wallet_expired|subscription_inactive/i)) return "subscription";
		if (has(/unknown_key|missing_key|malformed_key/i) || status === 401) return "reconnect";
		// Genuine upstream rate limit (the shared gateway 429s), not a billing 402.
		if (status === 429 || (status === undefined && /rate limit|too many requests|\b429\b/i.test(raw)))
			return "rate_limit";
		// A 4xx the model itself raised — unsupported tools/modality, context
		// overflow, bad request. Retrying the SAME model just re-fails, so this
		// kind offers a switch to the Recommended model rather than a plain retry.
		if (status === 400 || status === 404 || status === 422) return "model_error";
		return "generic";
	});

	const isBilling = $derived(
		["wallet_empty", "card_declined", "monthly_cap", "topup_disabled", "subscription"].includes(kind)
	);
	// "Warning" styling (amber) for soft, user-fixable states; hard error (red) otherwise.
	const isSoft = $derived(
		isBilling || kind === "rate_limit" || kind === "reconnect" || kind === "model_error"
	);
	// Can we actually offer the switch? Only when a fallback model was passed and
	// the error is a model-side one; otherwise fall back to a plain retry.
	const canSwitch = $derived(kind === "model_error" && !!onSwitchAndRetry && !!recommendedName);

	// Strip our "LLM error (status NNN): " wrapper and, when the remainder is the
	// provider's JSON error, surface just its human-readable message.
	const cleanMessage = $derived.by(() => {
		let msg = error?.message ?? "Something went wrong. Please try again.";
		msg = msg.replace(/^LLM error \(status \d{3}\):\s*/i, "");
		try {
			const j = JSON.parse(msg);
			const inner = j?.error?.message ?? j?.message;
			if (typeof inner === "string" && inner) return inner;
		} catch {
			// not JSON — leave as-is
		}
		return msg;
	});

	const COPY: Record<string, { title: string; message: string }> = {
		wallet_empty: {
			title: "Out of credits",
			message: "Your Virtues wallet is empty. Add credits to keep going — your monthly allotment refreshes on renewal.",
		},
		card_declined: {
			title: "Card declined",
			message: "We couldn't charge your card for an auto top-up. Update your payment method in Billing, then retry.",
		},
		monthly_cap: {
			title: "Monthly cap reached",
			message: "You've hit your monthly spend cap. Raise it in Settings → Billing, or wait until it resets.",
		},
		topup_disabled: {
			title: "Auto top-up off",
			message: "Auto top-up is disabled. Add credits manually in Billing, or set your own provider key.",
		},
		subscription: {
			title: "Subscription inactive",
			message: "Your subscription isn't active. Reconnect or update billing to continue.",
		},
		reconnect: {
			title: "Reconnect needed",
			message: "This box isn't recognized by billing. Reconnect your subscription to continue.",
		},
		rate_limit: {
			title: "Rate limit reached",
			message: "The AI provider is briefly rate-limiting. Wait a moment and retry.",
		},
	};

	const title = $derived(
		COPY[kind]?.title ??
			(kind === "model_error"
				? "This model couldn't handle that"
				: status
					? `Request failed (HTTP ${status})`
					: "An error occurred")
	);
	const billingMessage = $derived(COPY[kind]?.message);
</script>

{#if error}
	<div class="flex justify-start">
		<div
			class="error-container"
			class:rate-limit-error={isSoft}
		>
			<div class="error-icon">
				<Icon
					icon={isBilling ? "ri:wallet-3-line" : kind === "rate_limit" ? "ri:time-line" : kind === "reconnect" ? "ri:link" : kind === "model_error" ? "ri:shuffle-line" : "ri:error-warning-line"}
					width="20"
				/>
			</div>
			<div class="error-content">
				<div class="error-title">
					{title}
				</div>
				<div class="error-message">
					{billingMessage ?? cleanMessage}
				</div>
				<div class="error-actions">
					{#if isBilling}
						<a href="/billing" class="usage-link">
							<Icon icon="ri:wallet-3-line" width="16" />
							{kind === "wallet_empty" || kind === "topup_disabled" ? "Add credits" : "Manage billing"}
						</a>
					{:else if kind === "reconnect" || kind === "subscription"}
						<!-- The account gate lives on the getting-started page now,
						     which shows itself at the app root while unsatisfied. -->
						<a href="/" class="usage-link">
							<Icon icon="ri:link" width="16" />
							Reconnect subscription
						</a>
					{:else if canSwitch}
						<button
							type="button"
							class="retry-button"
							onclick={onSwitchAndRetry}
						>
							<Icon icon="ri:shuffle-line" width="16" />
							Switch to {recommendedName} &amp; retry
						</button>
					{:else}
						<button
							type="button"
							class="retry-button"
							onclick={onRetry}
						>
							<Icon icon="ri:refresh-line" width="16" />
							Retry
						</button>
					{/if}
				</div>
			</div>
		</div>
	</div>
{/if}

<style>
	@reference "../../../app.css";

	.error-container {
		display: flex;
		gap: 12px;
		padding: 16px;
		background: var(--color-error-subtle);
		border: 1px solid var(--color-error);
		border-radius: 12px;
		max-width: 600px;
	}

	.error-container.rate-limit-error {
		background: var(--color-warning-subtle);
		border-color: var(--color-warning);
	}

	.error-icon {
		flex-shrink: 0;
		color: var(--color-error);
	}

	.rate-limit-error .error-icon {
		color: var(--color-warning);
	}

	.error-content {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.error-title {
		font-weight: 600;
		color: var(--color-foreground);
	}

	.error-message {
		font-size: 14px;
		color: var(--color-foreground-muted);
		line-height: 1.5;
	}

	.error-actions {
		margin-top: 8px;
	}

	.retry-button,
	.usage-link {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 8px 12px;
		font-size: 13px;
		font-weight: 500;
		border-radius: 6px;
		cursor: pointer;
		transition: all 150ms ease;
	}

	.retry-button {
		background: var(--color-error);
		color: white;
		border: none;
	}

	.retry-button:hover {
		opacity: 0.9;
	}

	.usage-link {
		background: var(--color-warning);
		color: var(--color-foreground);
		text-decoration: none;
	}

	.usage-link:hover {
		opacity: 0.9;
	}
</style>
