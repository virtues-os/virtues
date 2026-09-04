<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Button, Input, Badge, SudoModal, Page } from '$lib';
	import UsageView from '$lib/components/tabs/views/UsageView.svelte';
	import { subscriptionStore } from '$lib/stores/subscription.svelte';
	import { openExternal } from '$lib/tauri/bridge';
	import { formatMicrosUSD, formatMicrosPrecise } from '$lib/utils/currency';
	import { formatDate } from '$lib/utils/dateUtils';
	import {
		getBillingLinkStatus,
		startBillingLink,
		openBillingPortal as requestBillingPortal,
		getBillingState,
		setBillingAutoTopup,
		getBillingUsage,
		getByoKey,
		setByoKey,
		deleteByoKey,
		ApiError,
	} from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import { toast } from 'svelte-sonner';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let portalLoading = $state(false);
	let portalError = $state<string | null>(null);
	// The branch, not the sentence. Several unrelated failures share one line
	// of prose ("Try again."), so the code beside it is the only thing a
	// screenshot can hand us. Shown verbatim — never translated, never mapped.
	let portalErrorCode = $state<string | null>(null);

	// Device-authorization link flow (connect a paid subscription). The box
	// never holds a Stripe key: we start a link, open the Atlas-hosted checkout
	// URL, and poll the box until it has picked up the billing token.
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

	// `subscribed`, not "do we hold a key". Those were the same thing until
	// 0017 decoupled linking from billing, after which a free account rendered
	// this whole page as though it were paying.
	const isSubscribed = $derived(subscriptionStore.subscribed);
	// atlas unreachable and nothing cached. Neither standing is honest here, so
	// the page says so instead of offering a subscription to someone who may
	// already have one.
	const standingUnknown = $derived(!subscriptionStore.entitlementKnown && !isSubscribed);

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
					// Re-fetch subscription state now that the box is linked.
					setTimeout(() => location.reload(), 1200);
				} else if (data.status === 'expired' || data.status === 'none') {
					stopPolling();
					linkPolling = false;
					linkInfo = null;
					linkError = 'The link expired before checkout completed — please try again.';
				}
			} catch {
				// transient; keep polling
			}
		}, intervalMs);
	}

	// Host the checkout tab will hand off to (e.g. atlas.virtues.com), surfaced
	// up front so the redirect isn't a surprise.
	const checkoutHost = $derived.by(() => {
		if (!linkInfo) return '';
		try {
			return new URL(linkInfo.verification_uri).host;
		} catch {
			return '';
		}
	});

	// Step 1: ask the box to start a link. We do NOT open the checkout tab here —
	// we show the code + destination first, then the user explicitly continues.
	async function connectSubscription() {
		linkLoading = true;
		linkError = null;
		linkDone = false;
		try {
			const data = await startBillingLink<LinkInfo & { error?: unknown }>();
			if (data.error) {
				linkError = typeof data.error === 'string' ? data.error : 'Failed to start subscription link';
				return;
			}
			linkInfo = data;
			// Poll from now on: covers both the "continue" button and the manual
			// enter-the-code path. The link self-expires (15 min) if unused.
			startPolling(Math.max((data.interval || 5) * 1000, 2000));
		} catch (e) {
			// Surface any server-provided error body (ApiError.message extracts it),
			// mirroring the old code that read data.error even on non-2xx.
			linkError = e instanceof ApiError ? e.message : 'Failed to connect to billing service';
		} finally {
			linkLoading = false;
		}
	}

	// Step 2: explicit hand-off to the Stripe-backed checkout, on user action.
	function proceedToCheckout() {
		if (linkInfo) openExternal(linkInfo.verification_uri_complete);
	}

	$effect(() => () => stopPolling());

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
				portalError = typeof data.error === 'string' ? data.error : data.error.message || 'Failed to open billing portal';
				portalErrorCode = data.code ?? null;
			}
		} catch (e) {
			// The endpoint answers 200 on every handled refusal, so landing here
			// means the box itself did not answer — the status is the only code
			// there is.
			portalError = e instanceof ApiError ? e.message : 'Failed to connect to billing service';
			portalErrorCode = e instanceof ApiError ? `http_${e.status}` : 'unreachable';
		} finally {
			portalLoading = false;
		}
	}

	// ─── Local billing-state (auto-top-up + BYO) ──────────────────────────
	type AutoTopupState = {
		enabled: boolean;
		failures_24h: number;
		disabled_at: string | null;
	};
	type ByoState = {
		configured: boolean;
		endpoint_url: string | null;
		/** Legacy label on credentials saved before the picker was removed. */
		provider: string | null;
		default_model: string | null;
	};
	type LocalBillingState = { auto_topup: AutoTopupState; byo: ByoState };

	let local = $state<LocalBillingState | null>(null);
	let localLoading = $state(true);
	// A failed read used to hide the whole chapter, so a box that could not
	// answer "is auto top-up on?" showed a page with no auto top-up on it —
	// indistinguishable from a box where the feature does not exist.
	let localError = $state<string | null>(null);

	async function loadLocal() {
		localLoading = true;
		localError = null;
		try {
			local = await getBillingState<LocalBillingState>();
		} catch (e) {
			localError = e instanceof ApiError ? e.message : 'Could not read this box’s wallet settings.';
		}
		localLoading = false;
	}

	async function setAutoTopup(enabled: boolean) {
		try {
			await setBillingAutoTopup(enabled);
			await loadLocal();
		} catch (e) {
			// The toggle is the one control here that changes money. A silent
			// no-op leaves the button showing the state you asked for and the
			// box in the state you didn't.
			toast.error(e instanceof ApiError ? e.message : 'Could not change auto top-up.');
			await loadLocal();
		}
	}

	$effect(() => { void loadLocal(); });

	// ─── BYO AI (inline management, formerly ByoKeyView) ───────────────────
	// The escape hatch from the Virtues wallet, and as of 2026-08-05 a whole
	// door rather than half of one: `BearerClient` diverts every `/v1/ai/*`
	// call — streaming and not — so chat, compaction, day summaries, image
	// generation and transcription all leave by the user's endpoint.
	//
	// The copy here is load-bearing and has been wrong in both directions
	// within a single day. It claimed "Skip the Virtues wallet" while seven
	// callers still billed it, then got rewritten to say nothing read the key
	// at all — from grepping `BYO_SOURCE_ID`, which the routing reaches
	// through `load_byo_credential` and never names. If you change this text,
	// verify against api/settings_byo.rs and virtues_api/client.rs, not grep.
	//
	// A credential is a URL and a key. There is no provider picker: one
	// contract (OpenAI-style /chat/completions + bearer), so only the address
	// varies. Example URLs live in the markup as *copy*, never as <option>s —
	// a stale doc line is wrong, a stale option is a broken feature, which is
	// precisely how `anthropic` and `google` shipped pointing at APIs we
	// cannot call. Plan of record: agents/plan/byo-ai-plan.md.
	//
	// Save and Delete are both sudo-gated (`change_byo_key` is one of the four
	// locked sensitive actions); the SudoModal handles the prompt + CLI
	// approval round-trip.
	type ByoStatus = {
		configured: boolean;
		/** Legacy label on credentials saved before the picker was removed. */
		provider: string | null;
		/** Slot name → the model id on the user's endpoint. */
		models: Record<string, string>;
		default_model: string | null;
		endpoint_url: string | null;
		created_at: string | null;
	};

	/** Host of an endpoint URL, for display. Falls back to the raw string. */
	function byoHost(url: string): string {
		try {
			return new URL(url).host;
		} catch {
			return url;
		}
	}

	let byoOpen = $state(false);
	let byoStatus = $state<ByoStatus | null>(null);
	let byoLoading = $state(false);
	let byoLoadError = $state<string | null>(null);

	// Form state. No provider field — a credential is a URL and a key.
	let byoApiKey = $state('');
	let byoEndpointUrl = $state('');
	/**
	 * One entry per slot, seeded blank.
	 *
	 * It started as `{}`, and `bind:value={byoModels[slot.key]}` in the form
	 * below then bound to `undefined` — which Svelte 5 REFUSES with
	 * `props_invalid_value`, thrown mid-render. The throw aborts the update,
	 * so the branch on screen never advances past "Loading…": every box
	 * without a key already saved opened Bring your own AI onto a panel that
	 * loads forever. The error only reaches the console, so the screen shows a
	 * plausible waiting state instead of a failure.
	 *
	 * `blankSlots()` rather than a literal so a new SLOT_FIELDS entry cannot
	 * reintroduce it.
	 */
	let byoModels = $state<Record<string, string>>({});

	/**
	 * The slot map form. Keys match the box's slot names exactly.
	 *
	 * `note` is where judgment about *which* model suits a role lives —
	 * deliberately here and not in the schema or the resolver, both of which
	 * treat all five slots identically. Model landscapes shift faster than we
	 * ship boxes, so guidance belongs somewhere a copy edit can fix.
	 */
	const SLOT_FIELDS = [
		{ key: 'chat', label: 'Chat', placeholder: 'x-ai/grok-4.5', note: '' },
		{ key: 'coding', label: 'Coding', placeholder: 'x-ai/grok-4.5', note: '' },
		{
			key: 'lite',
			label: 'Lite',
			placeholder: 'z-ai/glm-4.7',
			note: 'Titles, summaries, background jobs — high volume, so favor something cheap and quick.',
		},
		{ key: 'image', label: 'Image', placeholder: 'google/gemini-3-pro-image', note: '' },
		{
			key: 'omni',
			label: 'Audio',
			placeholder: 'google/gemini-3.5-flash',
			note: 'Transcription, and usually the largest line item. In practice Gemini 3 flash or flash-lite is the only workable choice: it has to hear muffled, in-pocket audio and reason about it, and the alternatives either reject audio outright or return words with no sense of the scene. Speech-to-text models will not do.',
		},
	] as const;

	function blankSlots(): Record<string, string> {
		return Object.fromEntries(SLOT_FIELDS.map((f) => [f.key, '']));
	}
	byoModels = blankSlots();

	// Sudo modal coordination — we mint a sudo request, then on approval we
	// fire the actual save/delete with the approved request id.
	let showSudoSave = $state(false);
	let showSudoDelete = $state(false);

	async function loadByo() {
		byoLoading = true;
		byoLoadError = null;
		try {
			byoStatus = await getByoKey<ByoStatus>();
			// Prefill so "Manage" edits the map instead of silently clearing it.
			// Merged OVER a blank map, never replacing it: a response that omits
			// a slot must leave that field bound to '' rather than undefined.
			byoModels = { ...blankSlots(), ...(byoStatus?.models ?? {}) };
		} catch (e) {
			byoLoadError = e instanceof Error ? e.message : 'Failed to load BYO status';
		} finally {
			byoLoading = false;
		}
	}

	function toggleByo() {
		byoOpen = !byoOpen;
		if (byoOpen && !byoStatus && !byoLoading) void loadByo();
	}

	function startByoSave() {
		if (!byoEndpointUrl.trim()) {
			toast.error('Paste the endpoint URL to send chat to');
			return;
		}
		if (!byoApiKey.trim()) {
			toast.error('Paste an API key first');
			return;
		}
		showSudoSave = true;
	}

	async function performByoSave(sudoRequestId: string) {
		try {
			// No `provider` — the box takes the URL as given. The field still
			// exists server-side to resolve credentials saved before the picker
			// was removed; nothing new should send it.
			// Blank rows are omitted, not sent as "": an absent slot means
			// "your endpoint uses our id", which is the right default.
			const models = Object.fromEntries(
				Object.entries(byoModels)
					.map(([k, v]) => [k, (v ?? '').trim()])
					.filter(([, v]) => v.length > 0),
			);
			await setByoKey({
				sudo_request_id: sudoRequestId,
				api_key: byoApiKey,
				endpoint_url: byoEndpointUrl,
				models,
			});
			toast.success('BYO key saved');
			byoApiKey = '';
			await Promise.all([loadByo(), loadLocal()]);
		} catch (e) {
			toast.error('Save failed', {
				description: e instanceof Error ? e.message : 'Unknown error',
			});
		}
	}

	function startByoDelete() {
		showSudoDelete = true;
	}

	async function performByoDelete(sudoRequestId: string) {
		try {
			await deleteByoKey(sudoRequestId);
			toast.success('BYO key removed — chat is back on your Virtues wallet');
			await Promise.all([loadByo(), loadLocal()]);
		} catch (e) {
			toast.error('Delete failed', {
				description: e instanceof Error ? e.message : 'Unknown error',
			});
		}
	}

	// ─── Wallet balance + recent ledger (proxied from virtues-api) ─────────
	type LedgerEntry = { ts: string; micros: number; kind: string; real_micros: number | null };
	type Usage = {
		balance_micros: number;
		month_to_date_micros: number;
		expires_at: string | null;
		entries: LedgerEntry[];
		error?: string;
	};
	let usage = $state<Usage | null>(null);
	let usageLoading = $state(true);
	/**
	 * Why the balance is missing, in the words the server used.
	 *
	 * This panel used to hide itself on any failure — which removed the one
	 * number the page exists to show and said nothing about why. A box whose
	 * wallet call is failing then looks identical to a box with no wallet, and
	 * the reader's only clue is an absence they have to notice.
	 */
	let usageError = $state<string | null>(null);

	async function loadUsage() {
		usageLoading = true;
		usageError = null;
		try {
			const data = await getBillingUsage<Usage>();
			// The server answers 200 with an `error` body when it cannot reach
			// the wallet, so a thrown error is not the only failure shape.
			if (data.error) {
				usageError = data.error;
				usage = null;
			} else {
				// Never trust `entries` to be present.
				usage = { ...data, entries: data.entries ?? [] };
			}
		} catch (e) {
			usageError = e instanceof ApiError ? e.message : 'Could not reach the wallet.';
			usage = null;
		}
		usageLoading = false;
	}

	$effect(() => { void loadUsage(); });

	const kindLabel: Record<string, string> = {
		grant: 'Monthly credit',
		topup: 'Top-up',
		charge: 'Usage',
		refund: 'Refund',
		adjust: 'Adjustment',
	};
	const kindIcon: Record<string, string> = {
		grant: 'ri:refresh-line',
		topup: 'ri:add-circle-line',
		charge: 'ri:sparkling-2-line',
		refund: 'ri:arrow-go-back-line',
		adjust: 'ri:equalizer-line',
	};

	// Human "renews" date from expiry.
	const renewsLabel = $derived(
		usage?.expires_at
			? formatDate(usage.expires_at, { month: 'long', day: 'numeric' })
			: null
	);
	function entryDate(ts: string): string {
		const d = new Date(ts);
		const today = new Date();
		const sameDay = d.toDateString() === today.toDateString();
		return sameDay
			? d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
			: formatDate(ts, { month: 'short', day: 'numeric' });
	}

	const statusLabel: Record<string, string> = {
		active: 'Active',
		trialing: 'Trial',
		past_due: 'Past Due',
		expired: 'Expired',
	};

	const statusColor: Record<string, string> = {
		active: 'text-success',
		trialing: 'text-info',
		past_due: 'text-warning',
		expired: 'text-error',
	};
</script>

<!--
	Billing — the room. There was a `PlanView.svelte` between this and
	SettingsView whose whole body was a <Page> wrapper and two child tags; it
	existed only to hold a title, and the title turned out to be wrong. Gone,
	and the wrapper lives here, where the chapters are.

	CHAPTERS, not cards. This was five identical `border rounded-lg p-6` boxes
	stacked down the page, which gave a two-line Subscription panel exactly the
	same weight as a BYO essay and made the balance — the one number anyone
	opens this page for — the third thing in the third box. The rest of the
	app (System especially) separates subjects with a hairline rule under a
	heading and reserves a bordered surface for places you ACT. That reads as
	hierarchy; five borders read as a list of equals.

	Order follows the account, not the code: what you are ON, what is LEFT
	(with the rule that refills it, and the ledger that moved it), where calls
	go INSTEAD, and the door out to the processor. The itemized call log is
	last, in UsageView — a statement puts the itemization at the end.
-->
<Page
	title="Billing"
	description="What AI costs you, and how it's paid for."
	maxWidth="wide"
>
<div class="plan-sections">
	<!-- ─── STANDING ──────────────────────────────────────────────────────
	     When there is no subscription, this slot is the connect flow instead:
	     linking the box is the whole job of the page in that state, and it
	     used to sit third, below a balance that cannot exist yet. -->
	{#if isSubscribed}
		<section class="chapter">
			<h2 class="settings-label">Standing</h2>
			<div class="ledger">
				<div class="ledger-row">
					<span class="ledger-label">Subscription</span>
					<span class="leader"></span>
					<span class="ledger-value {statusColor[subscriptionStore.status] || ''}">
						{statusLabel[subscriptionStore.status] || subscriptionStore.status}
					</span>
				</div>

				{#if subscriptionStore.status === 'trialing' && subscriptionStore.daysRemaining !== null}
					<div class="ledger-row">
						<span class="ledger-label">Trial ends</span>
						<span class="leader"></span>
						<span class="ledger-value"
							>{subscriptionStore.daysRemaining} day{subscriptionStore.daysRemaining === 1
								? ''
								: 's'} remaining</span
						>
					</div>
				{/if}

				{#if subscriptionStore.trialExpiresAt}
					<div class="ledger-row">
						<span class="ledger-label">Expiry date</span>
						<span class="leader"></span>
						<span class="ledger-value"
							>{formatDate(subscriptionStore.trialExpiresAt, {
								year: 'numeric',
								month: 'long',
								day: 'numeric',
							})}</span
						>
					</div>
				{/if}
			</div>
		</section>
	{:else if standingUnknown}
		<section class="chapter">
			<h2 class="settings-label">Standing</h2>
			<p class="chapter-lede">
				We couldn't reach the Virtues billing service just now, so this page can't say
				where your subscription stands. Your server is unaffected — nothing here gates it.
				Check your connection and reload.
			</p>
		</section>
	{:else}
		<section class="chapter">
			<h2 class="settings-label">Connect your subscription</h2>
			<p class="chapter-lede">
				Link this box to your Virtues subscription to turn on AI. Checkout happens on
				Stripe — your box never sees a payment key.
			</p>

			{#if linkError}
				<p class="note note-error">{linkError}</p>
			{/if}

			{#if linkDone}
				<p class="note note-success">Linked. Finishing setup…</p>
			{:else if linkInfo}
				<!-- Hand-off announced: what opens, where, and the pairing code,
				     shown before any tab opens. Continue is an explicit click. -->
				<div class="panel">
					<p class="panel-lede">
						Continue opens a new tab at
						<strong>{checkoutHost || 'the Virtues billing page'}</strong>
						to complete checkout on Stripe, then sends you back here automatically.
					</p>
					<div class="code-block">
						<div class="code-caption">Your pairing code</div>
						<div class="code-figure mono">{linkInfo.user_code}</div>
						<div class="code-caption">
							It should match the code shown on the checkout page. Expires in about 15
							minutes.
						</div>
					</div>
					<div>
						<Button variant="primary" onclick={proceedToCheckout}>
							Continue to checkout →
						</Button>
					</div>
					<p class="panel-foot">
						Prefer to do it by hand? Go to
						<span class="mono">{linkInfo.verification_uri}</span> and enter the code above.
					</p>
					{#if linkPolling}
						<p class="panel-foot">Waiting for checkout to complete…</p>
					{/if}
				</div>
			{:else}
				<div>
					<Button variant="primary" onclick={connectSubscription} disabled={linkLoading}>
						{linkLoading ? 'Starting…' : 'Connect subscription'}
					</Button>
				</div>
			{/if}
		</section>
	{/if}

	<!-- ─── BALANCE ───────────────────────────────────────────────────────
	     The figure, the rule that refills it, and the ledger that moved it —
	     one subject, one chapter. Auto top-up used to live two chapters below
	     the number it governs, in a panel called "Wallet & top-up" that a
	     reader had no reason to connect to the balance above it. -->
	<section class="chapter">
		<h2 class="settings-label">Balance</h2>

		{#if usageLoading && !usage && !usageError}
			<p class="chapter-lede">Reading the wallet…</p>
		{:else if usageError}
			<!-- The report stands where the number would be. An empty space is
			     not a report — and a placeholder dash at figure size read as a
			     stray rule, so the note takes the slot instead. -->
			<p class="note note-error note-figure">
				{usageError}
				<button class="link-btn" onclick={() => void loadUsage()}>Try again</button>
			</p>
		{:else if usage}
			<div class="figure-line">
				<span class="figure mono">{formatMicrosUSD(usage.balance_micros)}</span>
				<span class="figure-unit">available</span>
			</div>

			<!-- The standing sentence: what the number is, what happens when it
			     runs out, and when it refills. Three facts that were spread
			     across two cards and a toggle. -->
			<p class="standing">
				<!-- Three states, not two. `local` arrives on its own request, and
				     an `{:else}` here spent the first beat of every load asserting
				     "Auto top-up is off" — a false sentence about money, shown
				     while the truth was still in flight. Silence until known. -->
				{#if local}
					{#if local.auto_topup.enabled}
						$10 is charged to your card automatically when this reaches zero.
					{:else}
						Auto top-up is off — AI stops when this reaches zero.
					{/if}
				{/if}
				{#if renewsLabel}Renews {renewsLabel}.{/if}
			</p>

			<div class="ledger ledger-tight">
				<div class="ledger-row">
					<span class="ledger-label">Spent this month</span>
					<span class="leader"></span>
					<span class="ledger-value mono">{formatMicrosUSD(usage.month_to_date_micros)}</span>
				</div>
				{#if !localLoading && local}
					<div class="ledger-row">
						<span class="ledger-label">Auto top-up</span>
						<span class="leader"></span>
						<span class="ledger-value">
							<button
								class="toggle"
								class:on={local.auto_topup.enabled}
								aria-pressed={local.auto_topup.enabled}
								onclick={() => setAutoTopup(!local!.auto_topup.enabled)}
							>
								{local.auto_topup.enabled ? 'On' : 'Off'}
							</button>
						</span>
					</div>
				{/if}
			</div>

			{#if local && !local.auto_topup.enabled && local.auto_topup.disabled_at}
				<p class="note note-error">
					Auto top-up switched itself off after {local.auto_topup.failures_24h} failed charges
					in 24 hours. Update your payment method through Stripe below, then turn it back
					on.
				</p>
			{:else if local && local.auto_topup.failures_24h > 0}
				<p class="note note-warning">
					{local.auto_topup.failures_24h} failed top-up{local.auto_topup.failures_24h === 1
						? ''
						: 's'} in the last 24 hours. {3 - local.auto_topup.failures_24h} more before it
					switches itself off.
				</p>
			{/if}

			{#if localError}
				<p class="note note-error">{localError}</p>
			{/if}

			<!-- Money in and out. The itemized AI calls that make up the
			     `Usage` lines are the last chapter of the page; this is the
			     account statement, not the receipt. -->
			<h3 class="subhead">Wallet activity</h3>
			{#if usage.entries.length > 0}
				<div class="entries">
					{#each usage.entries.slice(0, 15) as e (e.ts + e.kind + e.micros)}
						<div class="entry">
							<span class="entry-kind">{kindLabel[e.kind] ?? e.kind}</span>
							<span class="leader"></span>
							<span class="entry-when mono">{entryDate(e.ts)}</span>
							<span class="entry-amount mono" class:credit={e.micros >= 0}>
								{e.micros < 0 ? '−' : '+'}{formatMicrosPrecise(Math.abs(e.micros))}
							</span>
						</div>
					{/each}
				</div>
			{:else}
				<p class="chapter-lede">Nothing has moved yet. Top-ups and charges appear here.</p>
			{/if}
		{/if}
	</section>

	<!-- ─── BRING YOUR OWN AI ─────────────────────────────────────────────
	     Its own chapter. It was a row nested inside the "Wallet & top-up"
	     card, three levels deep, and expanded inline into six paragraphs and
	     a nine-field form — the longest thing on the page, presented as a
	     sub-setting of a toggle. It is where every AI call goes instead, which
	     is a peer of the balance, not a detail of it. -->
	{#if !localLoading && local}
		<section class="chapter">
			<div class="chapter-head">
				<div>
					<h2 class="settings-label">Bring your own AI</h2>
					<p class="chapter-lede">
						{#if local.byo.configured}
							Every AI call leaves by {local.byo.endpoint_url
								? byoHost(local.byo.endpoint_url)
								: (local.byo.provider ?? 'your endpoint')}{#if local.byo.default_model}, defaulting
								to {local.byo.default_model}{/if}.
						{:else}
							Send every AI call to your own endpoint instead of the Virtues wallet.
						{/if}
					</p>
				</div>
				<button class="btn-quiet" onclick={toggleByo}>
					{byoOpen ? 'Close' : local.byo.configured ? 'Manage' : 'Set up'}
				</button>
			</div>

			{#if byoOpen}
				<div class="byo-body">
					<p class="prose-note">
						Point your box at any endpoint that speaks OpenAI-style
						<code>/chat/completions</code> with a bearer token. The key is stored on your
						box, encrypted, behind a sudo approval at the CLI.
					</p>
					<p class="prose-note">
						<strong>This covers every AI call</strong> — chat, compaction, day summaries,
						image generation and transcription all leave by your endpoint. The wallet stays
						live for the things that aren't AI and that your key can't pay for: maps, web
						search, photos, and bank connections.
					</p>
					<p class="prose-note warn">
						<strong>Consider whose key it is.</strong> Routing through an employer's account
						means your personal life passes through infrastructure they can read. Bringing
						your own key gives you control of the vendor and the bill; it does not, by
						itself, give you more privacy.
					</p>

					{#if byoLoading}
						<p class="chapter-lede">Loading…</p>
					{:else if byoLoadError}
						<p class="note note-error">{byoLoadError}</p>
					{:else if byoStatus?.configured}
						<div class="panel">
							<div class="panel-head">
								<div class="panel-mark"><Icon icon="ri:key-line" /></div>
								<div class="panel-body">
									<div class="panel-title">AI is going to your own endpoint</div>
									<div class="panel-facts">
										{#if byoStatus.endpoint_url}
											<!-- The host, not a chosen label: it says where traffic
											     actually goes, which a slug never did. -->
											<span>Sending to <Badge>{byoHost(byoStatus.endpoint_url)}</Badge></span>
											<span class="mono">{byoStatus.endpoint_url}</span>
										{/if}
										{#if byoStatus.default_model}
											<span>Default model <code>{byoStatus.default_model}</code></span>
										{/if}
									</div>
									<p class="panel-foot">
										Every AI call goes box → your endpoint, so Virtues is not in the path
										and your wallet isn't charged for it. The wallet stays live for maps,
										web search, photos and bank connections.
									</p>
								</div>
								<Button variant="ghost" onclick={startByoDelete}>
									<Icon icon="ri:close-circle-line" />
									Remove
								</Button>
							</div>
						</div>

						<div class="panel">
							<div class="panel-title">Replace the key</div>
							{@render byoKeyForm()}
							<div class="panel-actions">
								<Button variant="primary" onclick={startByoSave}>Save new key</Button>
							</div>
						</div>
					{:else}
						<div class="panel">
							<div class="panel-title">No key set</div>
							<p class="panel-lede">
								Everything routes through the Virtues wallet ($20/mo subscription plus
								usage). Set a key to send AI calls direct instead.
							</p>
							{@render byoKeyForm()}
							<div class="panel-actions">
								<Button variant="primary" onclick={startByoSave}>Save key</Button>
							</div>
						</div>
					{/if}

					<div class="byo-help">
						<p>
							<strong>A gateway is usually the best answer.</strong> Vercel AI Gateway,
							OpenRouter, LiteLLM or your work's proxy each reach every provider through
							one key — including models we pick that yours may not carry otherwise.
							Point this at theirs.
						</p>
						<p>
							Provider APIs work directly too: OpenAI, xAI, Groq, DeepSeek, Mistral, and
							Anthropic and Google on their OpenAI-compatible endpoints. So does a local
							Ollama, LM Studio or llama.cpp on <code>http://localhost</code>.
						</p>
						<p>
							<strong>AWS Bedrock needs a gateway in front.</strong> It signs requests
							rather than taking a bearer token, so a pasted key can't reach it — but every
							gateway above can.
						</p>
					</div>
				</div>
			{/if}
		</section>
	{/if}

	<!-- ─── PAYMENT ───────────────────────────────────────────────────────
	     The door out. Last, because it leaves. -->
	<section class="chapter">
		<div class="chapter-head">
			<div>
				<h2 class="settings-label">Payment</h2>
				<p class="chapter-lede">
					Your payment method, invoices, and plan changes are handled by Stripe.
				</p>
			</div>
			<button class="btn-quiet" onclick={openBillingPortal} disabled={portalLoading}>
				{portalLoading ? 'Opening…' : 'Open Stripe portal'}
			</button>
		</div>
		{#if portalError}
			<p class="note note-error">
				{portalError}
				{#if portalErrorCode}<span class="error-code">{portalErrorCode}</span>{/if}
			</p>
		{/if}
	</section>

	<!-- The itemization, last. -->
	<UsageView />
</div>
</Page>

<!--
	Endpoint URL, not a provider picker. There is no provider taxonomy: we
	speak one contract, so the only thing that varies is the address. The
	examples below are copy, deliberately — when a vendor moves a path, stale
	help text is wrong, whereas a stale <option> is a broken shipped feature.
	That is exactly how `anthropic` and `google` came to point at APIs we
	cannot call. See agents/plan/byo-ai-plan.md.
-->
{#snippet byoKeyForm()}
	<div class="space-y-3">
		<div>
			<label class="block text-xs text-foreground-muted mb-1" for="byo-url"
				>Endpoint URL</label
			>
			<Input
				id="byo-url"
				bind:value={byoEndpointUrl}
				placeholder="https://ai-gateway.vercel.sh/v1/chat/completions"
			/>
			<p class="text-xs text-foreground-muted mt-1.5">
				Any endpoint speaking OpenAI-style <code>/chat/completions</code> with a bearer token.
			</p>
		</div>
		<div>
			<label class="block text-xs text-foreground-muted mb-1" for="byo-key"
				>API key</label
			>
			<Input
				id="byo-key"
				type="password"
				bind:value={byoApiKey}
				placeholder="sk-…"
			/>
		</div>
		<div class="pt-1">
			<div class="text-xs font-medium mb-1">What your endpoint calls each model</div>
			<p class="text-xs text-foreground-muted mb-3">
				Optional. A model id is an address on one gateway, not a portable
				name — the same model we call <code>spacexai/grok-4.5</code>
				is <code>x-ai/grok-4.5</code> on
				OpenRouter. Leave a row blank if your endpoint uses the same ids we do,
				which is true for Vercel AI Gateway. Blank rows that your endpoint
				doesn't carry will fail with its own error naming the model.
			</p>
			<div class="space-y-2">
				{#each SLOT_FIELDS as slot (slot.key)}
					<div class="flex flex-col sm:flex-row sm:items-center gap-1 sm:gap-3">
						<label
							class="text-xs text-foreground-muted sm:w-28 sm:shrink-0 sm:text-right"
							for="byo-model-{slot.key}">{slot.label}</label
						>
						<div class="flex-1">
							<Input
								id="byo-model-{slot.key}"
								bind:value={byoModels[slot.key]}
								placeholder={slot.placeholder}
							/>
						</div>
					</div>
					{#if slot.note}
						<p class="text-xs text-foreground-muted sm:ml-[7.75rem] -mt-1">
							{slot.note}
						</p>
					{/if}
				{/each}
			</div>
		</div>
	</div>
{/snippet}

<SudoModal
	bind:show={showSudoSave}
	action="change_byo_key"
	title="Save BYO AI key"
	description="Sensitive action — chat will route through this key instead of the Virtues wallet. Background AI still bills the wallet. Confirm at the server's CLI."
	onApproved={performByoSave}
/>

<SudoModal
	bind:show={showSudoDelete}
	action="change_byo_key"
	title="Remove BYO AI key"
	description="Sensitive action — chat will switch back to the Virtues wallet. Confirm at the server's CLI."
	onApproved={performByoDelete}
/>

<style>
	/* ─── Chapters ──────────────────────────────────────────────────────────
	   Lifted from SystemInfoView so Plan and System read as siblings rather
	   than as two apps. A hairline under a small-caps eyebrow separates
	   subjects; a bordered surface is reserved for a place you ACT (`.panel`),
	   which is what makes the checkout hand-off and the BYO form look
	   different from the things you merely read. */
	.plan-sections {
		display: flex;
		flex-direction: column;
	}
	.chapter {
		padding-top: 28px;
		margin-top: 28px;
		border-top: 1px solid var(--color-border-subtle);
	}
	.chapter:first-child {
		border-top: none;
		margin-top: 8px;
		padding-top: 0;
	}
	/* Eyebrow, then a control on the same optical line — used where the
	   chapter is one sentence and one button (BYO, Payment). */
	.chapter-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
	}
	.chapter-lede {
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0;
		max-width: 60ch;
	}
	/* A step below `.settings-label`: still a heading, still roman serif, just
	   smaller — this names a part of Balance, not a chapter of the page. */
	.subhead {
		font-size: 0.875rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 22px 0 4px;
	}

	.mono {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-variant-numeric: tabular-nums;
	}

	/* ─── The figure ────────────────────────────────────────────────────────
	   The one number the page exists to show, set like System's vitals: mono,
	   large, tabular. It was `text-3xl font-semibold` sans, third down the
	   page, inside the second of five identical boxes. */
	.figure-line {
		display: flex;
		align-items: baseline;
		gap: 8px;
		margin: 2px 0 6px;
	}
	.figure {
		font-size: 40px;
		line-height: 1;
		font-weight: 500;
		color: var(--color-foreground);
	}
	.figure.dim {
		color: var(--color-foreground-subtle);
	}
	.figure-unit {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	/* What the number does next, in one sentence: the top-up rule and the
	   renewal date, which used to be a toggle two chapters away and a caption
	   in the corner of a card. */
	.standing {
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0;
		max-width: 60ch;
	}

	/* ─── Ledger rows (label · leader · value) ─────────────────────────────── */
	.ledger {
		margin-top: 10px;
	}
	.ledger-tight {
		margin-top: 14px;
	}
	.ledger-row {
		display: flex;
		align-items: baseline;
		gap: 8px;
		padding: 5px 0;
		font-size: 13px;
	}
	.ledger-label {
		color: var(--color-foreground-muted);
	}
	.leader {
		flex: 1;
		border-bottom: 1px dotted var(--color-border);
		transform: translateY(-3px);
		min-width: 16px;
	}
	.ledger-value {
		color: var(--color-foreground);
		white-space: nowrap;
	}

	/* ─── Wallet activity ───────────────────────────────────────────────────
	   Was an icon + label + date + amount row. The kind icon is gone: the sign
	   and the color already say which direction the money went, and a column
	   of five different glyphs down the left edge was the loudest thing in a
	   chapter about small numbers. */
	.entries {
		margin-top: 4px;
	}
	.entry {
		display: flex;
		align-items: baseline;
		gap: 10px;
		padding: 5px 0;
		font-size: 13px;
		border-bottom: 1px solid var(--color-border-subtle);
	}
	.entry:last-child {
		border-bottom: none;
	}
	.entry-kind {
		color: var(--color-foreground);
		white-space: nowrap;
	}
	.entry-when {
		font-size: 11px;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}
	.entry-amount {
		color: var(--color-foreground);
		white-space: nowrap;
		min-width: 5.5rem;
		text-align: right;
	}
	.entry-amount.credit {
		color: var(--color-success);
	}

	/* ─── Notes ─────────────────────────────────────────────────────────────
	   One shape for every thing the page has to tell you, tinted by weight.
	   The error variant is what stands where the balance would be when the
	   wallet cannot be read — the panel used to just vanish. */
	.note {
		font-size: 12px;
		line-height: 1.5;
		margin: 10px 0 0;
		padding: 9px 11px;
		border-radius: 6px;
		border: 1px solid transparent;
		max-width: 60ch;
	}
	.note-error {
		color: var(--color-error);
		border-color: color-mix(in srgb, var(--color-error) 22%, transparent);
		background: var(--color-error-subtle);
	}
	/* The branch, set quietly beside the sentence: an owner reads past it, and
	   reads it out loud when we ask what the screen said. */
	.error-code {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.85em;
		opacity: 0.72;
	}
	.note-warning {
		color: var(--color-warning);
		border-color: color-mix(in srgb, var(--color-warning) 22%, transparent);
		background: var(--color-warning-subtle);
	}
	/* Holds the balance figure's slot when there is no balance to show. */
	.note-figure {
		margin-top: 2px;
	}
	.note-success {
		color: var(--color-success);
		border-color: color-mix(in srgb, var(--color-success) 22%, transparent);
		background: var(--color-success-subtle);
	}
	.link-btn {
		background: none;
		border: none;
		padding: 0;
		margin-left: 6px;
		font: inherit;
		color: inherit;
		text-decoration: underline;
		text-underline-offset: 2px;
		cursor: pointer;
	}

	/* ─── Controls ──────────────────────────────────────────────────────────
	   Real actions use the shared Button (variant="primary"), because a
	   hand-rolled one here painted itself with `bg-accent text-on-accent` —
	   tokens this app has never defined. Tailwind emits no rule for an
	   undefined token and reports nothing, so every button on this page has
	   been shipping as unstyled text on a transparent ground: "Manage
	   Subscription" and "Connect subscription" both read as bare labels.
	   `bg-surface-alt`, used seven more times here, was the same mirage.

	   `.btn-quiet` stays local: it OPENS something (a portal, a form) and
	   should not carry the weight of the buttons that spend money. */
	.btn-quiet {
		flex-shrink: 0;
		padding: 5px 11px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
		transition: background 150ms ease;
	}
	.btn-quiet:hover { background: var(--color-background-hover); }
	.btn-quiet:disabled { opacity: 0.5; cursor: default; }

	.toggle {
		padding: 2px 10px;
		border: 1px solid var(--color-border);
		border-radius: 5px;
		background: var(--color-surface-elevated);
		color: var(--color-foreground-muted);
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
	}
	/* Engaged reads as INK, not as a color. Filled with the accent it was the
	   loudest thing on the page — louder than the balance it governs — and
	   on/off is a state, not a status. Weight carries it. */
	.toggle.on {
		background: var(--color-foreground);
		border-color: var(--color-foreground);
		color: var(--color-background);
	}

	/* ─── Panels: places you act ────────────────────────────────────────── */
	.panel {
		margin-top: 14px;
		padding: 18px;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: var(--color-surface);
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.panel-head { display: flex; align-items: flex-start; gap: 12px; }
	.panel-mark {
		flex-shrink: 0;
		width: 38px;
		height: 38px;
		border-radius: 9px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(in srgb, var(--color-success) 10%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-success) 30%, transparent);
		color: var(--color-success);
	}
	.panel-body { flex: 1; min-width: 0; }
	.panel-title { font-size: 14px; font-weight: 500; color: var(--color-foreground); }
	.panel-lede, .panel-foot {
		font-size: 12px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0;
	}
	.panel-facts {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 12px;
		font-size: 12px;
		color: var(--color-foreground-muted);
		margin-top: 4px;
	}
	.panel-actions { display: flex; justify-content: flex-end; }

	.code-block {
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: var(--color-surface-elevated);
		padding: 12px;
	}
	.code-caption { font-size: 11px; color: var(--color-foreground-muted); }
	.code-figure {
		font-size: 20px;
		font-weight: 500;
		letter-spacing: 0.12em;
		color: var(--color-foreground);
		margin: 3px 0;
	}

	/* ─── BYO prose ─────────────────────────────────────────────────────── */
	.byo-body { margin-top: 16px; }
	.prose-note {
		font-size: 12px;
		line-height: 1.6;
		color: var(--color-foreground-muted);
		margin: 0 0 12px;
		max-width: 68ch;
	}
	.prose-note.warn { color: var(--color-warning); }
	.byo-help {
		margin-top: 16px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.byo-help p {
		font-size: 12px;
		line-height: 1.6;
		color: var(--color-foreground-muted);
		margin: 0;
		max-width: 68ch;
	}

	code {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.92em;
		background: var(--color-surface-elevated);
		padding: 0 4px;
		border-radius: 3px;
	}

	@media (max-width: 640px) {
		.chapter-head { flex-direction: column; gap: 10px; }
		.figure { font-size: 34px; }
	}
</style>
