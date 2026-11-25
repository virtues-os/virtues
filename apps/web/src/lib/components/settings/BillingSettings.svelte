<script lang="ts">
	/**
	 * Settings · Billing: the balance at the top with how it is reached,
	 * then the subscription, auto top-up and invoices. What AI has
	 * cost is a page of its own under Models and spending; the balance links
	 * to it.
	 *
	 * The subscription has four states. Subscribed: the plan and Stripe.
	 * Linked with no subscription: Subscribe, never Connect, because
	 * connecting again would make a second account. Standing unknown (the
	 * billing service unreachable, nothing cached): say so rather than offer a
	 * subscription to someone who may have one. Not linked: the connect flow.
	 *
	 * Payments are Stripe's invoices, opened from here. The ledger this server
	 * can read is the last few dozen rows, mostly per-call charges, and a
	 * renewal row records the change in balance, so a list drawn from it would
	 * miss payments and show the wrong amounts.
	 */
	import { Button } from '$lib';
	import Switch from './Switch.svelte';
	import LoadError from './LoadError.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { subscriptionStore } from '$lib/stores/subscription.svelte';
	import { openExternal } from '$lib/tauri/bridge';
	import { formatMicrosUSD } from '$lib/utils/currency';
	import { formatDate } from '$lib/utils/dateUtils';
	import { balanceRows, type WalletUsage } from '$lib/settings/balance';
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
	import { listAccountServers, removeAccountServer, type AccountServer } from '$lib/api/servers';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import { toast } from 'svelte-sonner';
	import { reloadApp } from '$lib/reload';


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
	// link, you sign in with your email on the Atlas-hosted page, and this
	// polls until the server has its key. Subscribing comes after, from the
	// Plan row, because a checkout belongs to the account the server is on.
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
					linkError = 'The link expired before sign-in finished. Connect again for a new code.';
				}
			} catch {
				// transient; keep polling
			}
		}, intervalMs);
	}

	// The host the sign-in tab hands off to, named before it opens.
	const signinHost = $derived.by(() => {
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

	// Step 2: the hand-off to sign-in, on your click.
	function proceedToSignIn() {
		if (linkInfo) openExternal(linkInfo.verification_uri_complete);
	}

	$effect(() => () => stopPolling());

	// ─── Servers on this account ───────────────────────────────────────────
	// Shown only when another server shares the account: a list holding just
	// this one says nothing.
	let servers = $state<AccountServer[]>([]);
	let removingId = $state<string | null>(null);
	const otherServers = $derived(servers.filter((s) => !s.this_server));

	async function loadServers() {
		try {
			const r = await listAccountServers();
			servers = r.servers ?? [];
		} catch {
			servers = [];
		}
	}
	$effect(() => {
		if (subscriptionStore.linked) void loadServers();
	});

	async function removeServer(server: AccountServer) {
		const ok = await confirmAction({
			title: 'Remove this server?',
			body: `The server linked ${formatDate(server.linked_at, { month: 'long', day: 'numeric', year: 'numeric' })} stops using your Virtues account and its AI credit. It can be linked again later.`,
			confirmLabel: 'Remove server',
			danger: true,
		});
		if (!ok) return;
		removingId = server.id;
		try {
			const r = await removeAccountServer(server.id);
			if (r.ok) {
				toast.success('Server removed.');
				await loadServers();
			} else {
				toast.error(r.error ?? "Couldn't remove that server. Try again.");
			}
		} catch (e) {
			toast.error(e instanceof ApiError ? e.message : "Couldn't remove that server. Try again.");
		} finally {
			removingId = null;
		}
	}

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
	type Usage = WalletUsage & {
		error?: string;
		/** No Virtues account is connected, so there is no balance to read. */
		unlinked?: boolean;
	};
	let usage = $state<Usage | null>(null);
	let usageError = $state<string | null>(null);

	async function loadUsage() {
		usageError = null;
		try {
			const data = await getBillingUsage<Usage>();
			// The server answers 200 with an `error` body when it cannot reach
			// the wallet, so a thrown error is not the only failure shape.
			// Not connected: the subscription row above offers to connect, and
			// there is no balance to show yet.
			if (data.unlinked) {
				usage = null;
			} else if (data.error) {
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

	const day = (iso: string) => formatDate(iso, { month: 'long', day: 'numeric' });
	const rows = $derived(usage?.period_start ? balanceRows(usage, day(usage.period_start)) : []);
	const monthName = new Date().toLocaleDateString('en-US', { month: 'long', timeZone: 'UTC' });
	// The renewal date is Stripe's, from atlas. The wallet's `expires_at` is
	// the first of a month, whatever day the plan renews.
	const planHelp = $derived(
		[
			subscriptionStore.renewsAt ? `Renews ${day(subscriptionStore.renewsAt)}.` : '',
			'Includes $20 of AI and web search each month, plus bank data, places, maps and remote access. AI beyond that is sold at cost.',
		]
			.filter(Boolean)
			.join(' '),
	);

	function openCost() {
		windowShellStore.navigate('/virtues/assistant/spending/cost', { label: 'Settings · What AI has cost' });
	}
</script>

{#if usage}
	<div class="balance" id="setting-balance" data-setting="balance">
		<p class="figure"><span class="amount">{formatMicrosUSD(usage.balance_micros)}</span> <span class="unit">balance for AI and web search</span></p>
		{#if rows.length}
			<dl class="reached">
				{#each rows as row (row.label)}
					<dt>{row.label}</dt>
					<dd>{formatMicrosUSD(row.micros)}</dd>
				{/each}
				<dt class="total">Balance</dt>
				<dd class="total">{formatMicrosUSD(usage.balance_micros)}</dd>
			</dl>
		{/if}
		<p class="context">
			{#if !rows.length}
				{formatMicrosUSD(usage.month_to_date_micros)} used on AI and web search so far in {monthName}.
			{/if}
			{#if isSubscribed}
				Your plan credit resets to $20 when your plan renews. Top-up credit stays.
			{/if}
			<button type="button" class="link" onclick={openCost}>What AI has cost</button>
		</p>
	</div>
{:else if usageError && (isSubscribed || standingUnknown)}
	<div id="setting-balance" data-setting="balance">
		<LoadError message={`Your server couldn't read your balance. ${usageError}`} onretry={() => void loadUsage()} />
	</div>
{/if}

<section class="group">
	<h2 class="settings-label">Subscription</h2>
	{#if isSubscribed}
		<SettingRow id="subscription" label="Plan" help={planHelp}>
			<div class="plan">
				<span class="plan-name">Virtues, $29 a month</span>
				<Button variant="secondary" onclick={openBillingPortal} loading={portalLoading}>Manage in Stripe</Button>
			</div>
		</SettingRow>
		{#if portalError}
			<p class="note note-error">
				{portalError}
				{#if portalErrorCode}<span class="error-code">{portalErrorCode}</span>{/if}
			</p>
		{/if}
		{#if subscriptionStore.email}
			<SettingRow id="billed-to" label="Billed to">
				<span class="plan-name">{subscriptionStore.email}</span>
			</SettingRow>
		{/if}
	{:else if subscriptionStore.linked && subscriptionStore.entitlementKnown}
		<SettingRow
			id="subscription"
			label="Plan"
			help="None yet. $29 a month. Includes $20 of AI and web search each month, plus bank data, places, maps and remote access. AI beyond that is sold at cost."
		>
			{#if subscribeWaiting}
				<span class="waiting">Waiting for checkout…</span>
			{:else}
				<Button variant="secondary" onclick={openSubscribe} loading={subscribeLoading}>Subscribe, $29 a month</Button>
			{/if}
		</SettingRow>
		{#if subscribeError}
			<p class="note note-error">
				{subscribeError}
				{#if subscribeErrorCode}<span class="error-code">{subscribeErrorCode}</span>{/if}
			</p>
		{/if}
	{:else if standingUnknown}
		<SettingRow id="subscription" label="Plan">
			<p class="standing">
				Your server couldn't reach the Virtues billing service, so it can't say where your subscription stands. It keeps
				working either way. Check your connection and reload.
			</p>
		</SettingRow>
	{:else}
		<SettingRow
			id="subscription"
			label="Connect your Virtues account"
			help="Sign in with your email. A new address creates your account, and you can subscribe once your server is connected. Your server never sees a payment key."
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
						<strong>{signinHost || 'the Virtues sign-in page'}</strong>
						where you enter your email. Open the link we send and choose Attach box, and this page picks up on its own.
					</p>
					<div class="code-block">
						<div class="code-caption">Your pairing code</div>
						<div class="code-figure mono">{linkInfo.user_code}</div>
						<div class="code-caption">It should match the code on the sign-in page. It expires in about 15 minutes.</div>
					</div>
					<div><Button variant="primary" onclick={proceedToSignIn}>Continue to sign in →</Button></div>
					<p class="panel-foot">
						Prefer to do it by hand? Go to <span class="mono">{linkInfo.verification_uri}</span> and enter the code above.
					</p>
					{#if linkPolling}<p class="panel-foot">Waiting for you to open the emailed link…</p>{/if}
				</div>
			{:else}
				<div>
					<Button variant="primary" onclick={connectSubscription} loading={linkLoading}>Connect your account</Button>
				</div>
			{/if}
		</SettingRow>
	{/if}

	{#if autoTopup}
		<SettingRow
			id="auto-top-up"
			label="Auto top-up"
			help={autoTopup.enabled
				? 'When your balance runs out, Virtues charges $20 to your card and adds $19.12 to your balance. The $0.88 difference is the card fee.'
				: 'Off. AI stops when your balance runs out, until your plan renews.'}
		>
			<Switch checked={autoTopup.enabled} label="Auto top-up" onchange={(next) => setAutoTopup(next)} />
		</SettingRow>
		{#if !autoTopup.enabled && autoTopup.disabled_at}
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
		<SettingRow id="auto-top-up" label="Auto top-up" help="Your server couldn't read this setting. {localError}">
			<Button variant="secondary" onclick={() => void loadLocal()}>Try again</Button>
		</SettingRow>
	{/if}

	{#if otherServers.length > 0}
		<SettingRow
			id="account-servers"
			label="Servers on this account"
			help="Each of these can use your Virtues account and its AI credit. Remove one you don't recognize."
			block
		>
			<ul class="servers">
				<li><span>This server</span></li>
				{#each otherServers as server (server.id)}
					<li>
						<span>Server linked {formatDate(server.linked_at, { month: 'long', day: 'numeric', year: 'numeric' })}</span>
						<Button variant="secondary" onclick={() => void removeServer(server)} loading={removingId === server.id}>Remove</Button>
					</li>
				{/each}
			</ul>
		</SettingRow>
	{/if}

	{#if isSubscribed}
		<SettingRow id="invoices" label="Payments" help="Your monthly charges and top-ups, each with its receipt.">
			<Button variant="secondary" onclick={openBillingPortal} loading={portalLoading}>See payments in Stripe</Button>
		</SettingRow>
	{/if}
</section>

<style>
	.servers {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.servers li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		min-height: 34px;
	}
	.mono {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-variant-numeric: tabular-nums;
	}

	/* The balance: the figure first, then how it is reached. */
	.balance {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 16px 0 8px;
	}
	.figure {
		margin: 0;
		display: flex;
		align-items: baseline;
		gap: 12px;
	}
	.amount {
		font-family: var(--font-serif);
		font-size: var(--text-title);
		line-height: 1.1;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground);
	}
	.unit {
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
	}
	.reached {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 6px 24px;
		max-width: 420px;
		margin: 0;
		font-size: var(--text-label);
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
	}
	.reached dd {
		margin: 0;
		text-align: right;
	}
	.reached .total {
		padding-top: 6px;
		color: var(--color-foreground);
	}
	.context {
		margin: 0;
		font-size: var(--text-small);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
	.link:hover {
		text-decoration: underline;
	}

	.group {
		padding: 24px 0 8px;
	}

	.plan {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		width: 100%;
	}
	.plan-name {
		font-size: var(--text-label);
		color: var(--color-foreground);
	}
	.standing {
		margin: 6px 0 0;
		font-size: var(--text-small);
		line-height: 1.45;
		color: var(--color-foreground-muted);
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

</style>
