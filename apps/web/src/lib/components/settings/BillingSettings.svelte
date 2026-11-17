<script lang="ts">
	/**
	 * Settings → Billing, the rows: where your subscription stands, the
	 * balance, and auto top-up. What AI has cost and bringing your own AI are
	 * pages of their own (BillingView, ByoPage); the spending limits follow
	 * these rows on the page.
	 *
	 * The subscription row has four states. Subscribed: the plan and Stripe.
	 * Linked with no subscription: Subscribe, never Connect, because
	 * connecting again would make a second account. Standing unknown (the
	 * billing service unreachable, nothing cached): say so rather than offer a
	 * subscription to someone who may have one. Not linked: the connect flow.
	 */
	import { Button } from '$lib';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import { settingsFilter } from '$lib/settings/filter';
	import { subscriptionStore } from '$lib/stores/subscription.svelte';
	import { openExternal } from '$lib/tauri/bridge';
	import { formatMicrosPrecise } from '$lib/utils/currency';
	import { formatDate } from '$lib/utils/dateUtils';
	import {
		getBillingLinkStatus,
		startBillingLink,
		openBillingPortal as requestBillingPortal,
		subscribeBilling,
		getBillingState,
		setBillingAutoTopup,
		getBillingUsage,
		ApiError,
	} from '$lib/api/client';
	import { toast } from 'svelte-sonner';
	import { reloadApp } from '$lib/reload';

	// A note under a row hides with it while the page is filtered.
	const filter = settingsFilter();

	const isSubscribed = $derived(subscriptionStore.subscribed);
	const standingUnknown = $derived(!subscriptionStore.entitlementKnown && !isSubscribed);

	// ─── Stripe's portal ───────────────────────────────────────────────────
	let portalLoading = $state(false);
	let portalError = $state<string | null>(null);
	// The branch, not the sentence: several failures share one line of prose,
	// so the code beside it is what a screenshot can hand us. Shown verbatim.
	let portalErrorCode = $state<string | null>(null);

	async function openBillingPortal() {
		portalLoading = true;
		portalError = null;
		portalErrorCode = null;
		try {
			const data = await requestBillingPortal<{
				url?: string;
				error?: { message?: string } | string;
				code?: string;
			}>();
			if (data.url) {
				openExternal(data.url);
			} else if (data.error) {
				portalError = typeof data.error === 'string' ? data.error : data.error.message || 'Could not open Stripe.';
				portalErrorCode = data.code ?? null;
			}
		} catch (e) {
			// The endpoint answers 200 on every handled refusal, so landing here
			// means the server itself did not answer.
			portalError = e instanceof ApiError ? e.message : 'Could not reach the Virtues billing service.';
			portalErrorCode = e instanceof ApiError ? `http_${e.status}` : 'unreachable';
		} finally {
			portalLoading = false;
		}
	}

	// ─── Subscribe (a linked account with no subscription) ─────────────────
	let subscribeLoading = $state(false);
	let subscribeError = $state<string | null>(null);
	let subscribeErrorCode = $state<string | null>(null);
	let subscribeWaiting = $state(false);
	let subscribePoll: ReturnType<typeof setInterval> | null = null;

	function stopSubscribePoll() {
		if (subscribePoll) {
			clearInterval(subscribePoll);
			subscribePoll = null;
		}
		subscribeWaiting = false;
	}

	async function openSubscribe() {
		subscribeLoading = true;
		subscribeError = null;
		subscribeErrorCode = null;
		try {
			const data = await subscribeBilling<{ url?: string; error?: string; code?: string }>();
			if (data.url) {
				openExternal(data.url);
				// Ten minutes of fresh checks, every 5s: long enough for a card and
				// a 3-D Secure prompt, short enough not to poll an abandoned tab.
				subscribeWaiting = true;
				let ticks = 0;
				subscribePoll = setInterval(async () => {
					ticks += 1;
					await subscriptionStore.check(true);
					if (subscriptionStore.subscribed || ticks >= 120) stopSubscribePoll();
				}, 5000);
			} else if (data.error) {
				subscribeError = data.error;
				subscribeErrorCode = data.code ?? null;
			}
		} catch (e) {
			subscribeError = e instanceof ApiError ? e.message : 'Could not reach the Virtues billing service.';
			subscribeErrorCode = e instanceof ApiError ? `http_${e.status}` : 'unreachable';
		} finally {
			subscribeLoading = false;
		}
	}
	$effect(() => () => stopSubscribePoll());

	// ─── Connect (no account linked yet) ───────────────────────────────────
	// Device authorization: the server never holds a Stripe key. It starts a
	// link, you finish checkout on the Atlas-hosted page, and this polls until
	// the server has the billing token.
	type LinkInfo = {
		user_code: string;
		verification_uri: string;
		verification_uri_complete: string;
		interval: number;
	};
	let linkLoading = $state(false);
	let linkError = $state<string | null>(null);
	let linkInfo = $state<LinkInfo | null>(null);
	let linkPolling = $state(false);
	let linkDone = $state(false);
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	function stopPolling() {
		if (pollTimer) {
			clearInterval(pollTimer);
			pollTimer = null;
		}
	}

	function startPolling(intervalMs: number) {
		linkPolling = true;
		stopPolling();
		pollTimer = setInterval(async () => {
			try {
				const data = await getBillingLinkStatus<{ status: string }>();
				if (data.status === 'ready') {
					stopPolling();
					linkPolling = false;
					linkDone = true;
					setTimeout(() => reloadApp(), 1200);
				} else if (data.status === 'expired' || data.status === 'none') {
					stopPolling();
					linkPolling = false;
					linkInfo = null;
					linkError = 'The link expired before checkout finished. Connect again for a new code.';
				}
			} catch {
				// transient; keep polling
			}
		}, intervalMs);
	}

	// The host the checkout tab hands off to, named before it opens.
	const checkoutHost = $derived.by(() => {
		if (!linkInfo) return '';
		try {
			return new URL(linkInfo.verification_uri).host;
		} catch {
			return '';
		}
	});

	// Step 1: start a link and show the code and destination. No tab opens yet.
	async function connectSubscription() {
		linkLoading = true;
		linkError = null;
		linkDone = false;
		try {
			const data = await startBillingLink<LinkInfo & { error?: unknown }>();
			if (data.error) {
				linkError = typeof data.error === 'string' ? data.error : 'Could not start the link to your Virtues account.';
				return;
			}
			linkInfo = data;
			// Covers both Continue and typing the code by hand. The link expires
			// on its own after 15 minutes.
			startPolling(Math.max((data.interval || 5) * 1000, 2000));
		} catch (e) {
			linkError = e instanceof ApiError ? e.message : 'Could not reach the Virtues billing service.';
		} finally {
			linkLoading = false;
		}
	}

	// Step 2: the hand-off to checkout, on your click.
	function proceedToCheckout() {
		if (linkInfo) openExternal(linkInfo.verification_uri_complete);
	}

	$effect(() => () => stopPolling());

	// ─── Auto top-up ───────────────────────────────────────────────────────
	type AutoTopupState = { enabled: boolean; failures_24h: number; disabled_at: string | null };
	let autoTopup = $state<AutoTopupState | null>(null);
	let localError = $state<string | null>(null);

	async function loadLocal() {
		localError = null;
		try {
			autoTopup = (await getBillingState<{ auto_topup: AutoTopupState }>()).auto_topup;
		} catch (e) {
			localError = e instanceof ApiError ? e.message : 'Could not read your server’s wallet settings.';
		}
	}

	async function setAutoTopup(enabled: boolean) {
		try {
			await setBillingAutoTopup(enabled);
		} catch (e) {
			// This switch changes money. A silent no-op would leave it showing
			// the state you asked for and the server in the one you didn't.
			toast.error(e instanceof ApiError ? e.message : 'Could not change auto top-up.');
		}
		await loadLocal();
	}

	$effect(() => {
		void loadLocal();
	});

	// ─── Balance ───────────────────────────────────────────────────────────
	type Usage = { balance_micros: number; month_to_date_micros: number; expires_at: string | null; error?: string };
	let usage = $state<Usage | null>(null);
	let usageError = $state<string | null>(null);

	async function loadUsage() {
		usageError = null;
		try {
			const data = await getBillingUsage<Usage>();
			// The server answers 200 with an `error` body when it cannot reach
			// the wallet, so a thrown error is not the only failure shape.
			if (data.error) {
				usageError = data.error;
				usage = null;
			} else {
				usage = data;
			}
		} catch (e) {
			usageError = e instanceof ApiError ? e.message : 'Could not reach the wallet.';
			usage = null;
		}
	}

	$effect(() => {
		void loadUsage();
	});

	const renewsLabel = $derived(
		usage?.expires_at ? formatDate(usage.expires_at, { month: 'long', day: 'numeric' }) : null,
	);
	const subscribedHelp = $derived(
		['Virtues, $20 a month.', renewsLabel ? `Renews ${renewsLabel}.` : '', subscriptionStore.email ? `On ${subscriptionStore.email}.` : '']
			.filter(Boolean)
			.join(' '),
	);
</script>

{#if isSubscribed}
	<SettingRow id="subscription" label="Subscription" help={subscribedHelp}>
		<button class="btn-quiet" onclick={openBillingPortal} disabled={portalLoading}>
			{portalLoading ? 'Opening…' : 'Manage in Stripe'}
		</button>
	</SettingRow>
	{#if portalError && filter.shows('subscription')}
		<p class="note note-error">
			{portalError}
			{#if portalErrorCode}<span class="error-code">{portalErrorCode}</span>{/if}
		</p>
	{/if}
{:else if subscriptionStore.linked && subscriptionStore.entitlementKnown}
	<SettingRow
		id="subscription"
		label="Subscription"
		help="None yet. One subscription covers hosted AI, web search, places and bank data for $20 a month. Your own AI key covers the first on its own."
	>
		{#if subscribeWaiting}
			<span class="waiting">Waiting for checkout…</span>
		{:else}
			<button class="btn-quiet" onclick={openSubscribe} disabled={subscribeLoading}>
				{subscribeLoading ? 'Opening…' : 'Subscribe · $20/mo'}
			</button>
		{/if}
	</SettingRow>
	{#if subscribeError && filter.shows('subscription')}
		<p class="note note-error">
			{subscribeError}
			{#if subscribeErrorCode}<span class="error-code">{subscribeErrorCode}</span>{/if}
		</p>
	{/if}
{:else if standingUnknown}
	<SettingRow
		id="subscription"
		label="Subscription"
		help="Your server couldn't reach the Virtues billing service, so it can't say where your subscription stands. It keeps working either way. Check your connection and reload."
	>
		<span></span>
	</SettingRow>
{:else}
	<SettingRow
		id="subscription"
		label="Connect your Virtues account"
		help="Sign in if you have an account. Creating one takes you through Stripe for the subscription. Your server never sees a payment key."
		block
	>
		{#if linkError}
			<p class="note note-error">{linkError}</p>
		{/if}
		{#if linkDone}
			<p class="note note-success">Linked. Finishing setup…</p>
		{:else if linkInfo}
			<!-- What opens, where, and the pairing code, shown before any tab
			     opens. Continue is an explicit click. -->
			<div class="panel">
				<p class="panel-lede">
					Continue opens a new tab at
					<strong>{checkoutHost || 'the Virtues billing page'}</strong>
					to finish checkout on Stripe. It sends you back here when you're done.
				</p>
				<div class="code-block">
					<div class="code-caption">Your pairing code</div>
					<div class="code-figure mono">{linkInfo.user_code}</div>
					<div class="code-caption">It should match the code on the checkout page. It expires in about 15 minutes.</div>
				</div>
				<div><Button variant="primary" onclick={proceedToCheckout}>Continue to checkout →</Button></div>
				<p class="panel-foot">
					Prefer to do it by hand? Go to <span class="mono">{linkInfo.verification_uri}</span> and enter the code above.
				</p>
				{#if linkPolling}<p class="panel-foot">Waiting for checkout to finish…</p>{/if}
			</div>
		{:else}
			<div>
				<Button variant="primary" onclick={connectSubscription} disabled={linkLoading}>
					{linkLoading ? 'Starting…' : 'Connect subscription'}
				</Button>
			</div>
		{/if}
	</SettingRow>
{/if}

{#if usage}
	<SettingRow
		id="balance"
		label="Balance"
		help="What's left for AI. {formatMicrosPrecise(usage.month_to_date_micros)} spent so far this month."
	>
		<span class="value mono">{formatMicrosPrecise(usage.balance_micros)}</span>
	</SettingRow>
{:else if usageError && (isSubscribed || standingUnknown)}
	<SettingRow id="balance" label="Balance" help="Your server couldn't read the balance.">
		<button class="btn-quiet" onclick={() => void loadUsage()}>Check again</button>
	</SettingRow>
	{#if filter.shows('balance')}<p class="note note-error"><span class="error-code">{usageError}</span></p>{/if}
{/if}

{#if autoTopup}
	<SettingRow
		id="auto-top-up"
		label="Auto top-up"
		help={autoTopup.enabled ? 'Adds $10 when your balance reaches $0.' : 'Off. AI stops when your balance reaches $0.'}
	>
		<button
			class="switch"
			class:on={autoTopup.enabled}
			role="switch"
			aria-checked={autoTopup.enabled}
			aria-label="Auto top-up"
			onclick={() => setAutoTopup(!autoTopup!.enabled)}
		></button>
	</SettingRow>
	{#if !filter.shows('auto-top-up')}
		<!-- hidden with its row -->
	{:else if !autoTopup.enabled && autoTopup.disabled_at}
		<p class="note note-error">
			Auto top-up turned itself off after charges failed three times in a day. Update your payment
			method in Stripe, then turn it back on.
		</p>
	{:else if autoTopup.failures_24h > 0}
		<p class="note note-warning">
			A top-up charge failed in the last day. After three failures it turns itself off.
		</p>
	{/if}
{:else if localError}
	<SettingRow id="auto-top-up" label="Auto top-up" help="Your server couldn't read this setting.">
		<button class="btn-quiet" onclick={() => void loadLocal()}>Check again</button>
	</SettingRow>
	{#if filter.shows('auto-top-up')}<p class="note note-error"><span class="error-code">{localError}</span></p>{/if}
{/if}

<style>
	.mono {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-variant-numeric: tabular-nums;
	}
	.value {
		font-size: 15px;
		color: var(--color-foreground);
	}
	.waiting {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	/* One shape for everything the rows have to tell you, tinted by weight. */
	.note {
		font-size: 12px;
		line-height: 1.5;
		margin: 8px 0;
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
	.note-warning {
		color: var(--color-warning);
		border-color: color-mix(in srgb, var(--color-warning) 22%, transparent);
		background: var(--color-warning-subtle);
	}
	.note-success {
		color: var(--color-success);
		border-color: color-mix(in srgb, var(--color-success) 22%, transparent);
		background: var(--color-success-subtle);
	}
	/* The branch, set quietly beside the sentence. */
	.error-code {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.85em;
		opacity: 0.72;
	}

	/* Opens something (a portal, a checkout); lighter than a button that
	   spends money. */
	.btn-quiet {
		flex-shrink: 0;
		padding: 4px 12px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
		transition: background 150ms ease;
	}
	.btn-quiet:hover {
		background: var(--color-background-hover);
	}
	.btn-quiet:disabled {
		opacity: 0.5;
		cursor: default;
	}

	/* The connect hand-off: a place you act. */
	.panel {
		padding: 16px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.panel-lede,
	.panel-foot {
		font-size: 12px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0;
	}
	.code-block {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		background: var(--color-surface-elevated);
		padding: 12px;
	}
	.code-caption {
		font-size: 11px;
		color: var(--color-foreground-muted);
	}
	.code-figure {
		font-size: 20px;
		font-weight: 500;
		letter-spacing: 0.12em;
		color: var(--color-foreground);
		margin: 4px 0;
	}

	.switch {
		width: 44px;
		height: 26px;
		border-radius: var(--radius-full);
		background: var(--color-border);
		position: relative;
		flex: none;
		border: 0;
		cursor: pointer;
		transition: background 150ms ease;
		padding: 0;
	}
	.switch.on {
		background: var(--color-success);
	}
	.switch::after {
		content: '';
		position: absolute;
		top: 3px;
		left: 3px;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: #fff;
		box-shadow: 0 1px 2px rgba(0, 0, 0, 0.18);
		transition: transform 150ms ease;
	}
	.switch.on::after {
		transform: translateX(18px);
	}
	.switch:focus-visible {
		outline: 2px solid var(--color-foreground);
		outline-offset: 3px;
	}
</style>
