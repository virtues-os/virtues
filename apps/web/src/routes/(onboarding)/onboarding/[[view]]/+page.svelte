<!--
  /onboarding — six screens, one at a time.

  WAS A DOCUMENT, IS NOW A SEQUENCE. The first build was a single editorial
  scroll with a table of contents in the left margin: you read top to bottom and
  each finished action streamed the next chapter in. The idea was that the form
  should be the argument — Virtues writes a document from your life, so watch
  one write itself.

  It did not survive contact with the flow. Two of the chapters (the letter, the
  interview) had already been pulled out onto surfaces of their own, because
  reading and writing are not the same grammar as working; what was left was a
  scroll of three sections wearing the costume of a document. Meanwhile the
  chapters that WERE on their own screens had no way to say where they sat in
  the whole, so someone four screens in could not tell whether they were near
  the start or the end. That is the commonest reason people abandon a flow they
  have already paid for.

  So: every step is a screen, every screen wears OnboardingHeader, and the strip
  in that header is the only progress indicator. It replaced the left rail,
  which could only ever appear on the one surface long enough to scroll.

    ① letter    the founder's letter                     (in-memory, once a session)
    ② names     two names, thirty seconds
    ③ account   the one required gate
    ④ sources   connect what already holds your life     (skippable)
    ⑤ words     the interview — its own surface, and the draft after it
    ⑥ you       the reveal

  THE URL IS THE FLOW. Each view is `/onboarding/<slug>`, so Back and Forward
  work, a refresh keeps your place, and a screen can be linked to. Five local
  booleans used to encode this between them; they made Back leave the app
  entirely and a refresh start the step over.

  Server state still decides what is REACHABLE — everything past `account`
  needs the account — so a hand-typed `/onboarding/you` on an unlinked box is
  bounced rather than honored.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import { page } from "$app/state";
	import { onMount } from "svelte";
	import { fade } from "svelte/transition";
	import { Button } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import {
		getProfile,
		updateProfile,
		getSetupState,
		skipOnboarding,
		getInterviewAnswers,
	} from "$lib/api/client";
	import OnboardingHeader from "$lib/components/onboarding/OnboardingHeader.svelte";
	import {
		STEPS,
		VIEW_ORDER,
		VIEW_STEP,
		STEP_VIEW,
		isViewId,
		type StepId,
		type ViewId,
	} from "$lib/components/onboarding/steps";
	import AccountGate from "$lib/components/onboarding/document/AccountGate.svelte";
	import FoundersLetter from "$lib/components/onboarding/document/FoundersLetter.svelte";
	import Introductions from "$lib/components/onboarding/document/Introductions.svelte";
	import Interview from "$lib/components/onboarding/interview/Interview.svelte";
	import DraftReview from "$lib/components/onboarding/interview/DraftReview.svelte";
	import ConnectWorld from "$lib/components/onboarding/document/ConnectWorld.svelte";
	import RevealSection from "$lib/components/onboarding/document/RevealSection.svelte";
	import Modal from "$lib/components/Modal.svelte";

	type Step = { id: string; title: string; done: boolean; detail?: string; kind?: string };
	type SetupState = {
		setup: Step[];
		setup_complete: boolean;
		onboarding: Step[];
		onboarding_complete: boolean;
		onboarding_status: string;
	};

	let state_ = $state<SetupState | null>(null);
	let loading = $state(true);
	let mode = $state<"document" | "manual">("document");
	let advancedOpen = $state(false);
	let reduced = $state(false);

	// ── the URL is the flow ───────────────────────────────────────────
	//
	// `letterRead`, `introDone`, `screen`, `interviewOpen` and `draftOpen` were
	// five booleans that between them encoded one fact: which screen you are on.
	// Being local, they made Back leave the app entirely, a refresh lose your
	// place, and nothing linkable. The address bar holds that fact better than
	// any of them, and it comes with history for free.
	const view = $derived<ViewId>(isViewId(page.params.view) ? page.params.view : "letter");
	const step = $derived<StepId>(VIEW_STEP[view]);

	// Which way to turn the page. Derived by comparing positions rather than set
	// by whoever called the navigation, so the browser's own Back and Forward
	// buttons animate correctly too — they never run our handlers.
	let seen = $state<number>(VIEW_ORDER.indexOf("letter"));
	let back = $state(false);
	$effect(() => {
		const at = VIEW_ORDER.indexOf(view);
		back = at < seen;
		seen = at;
	});

	/** Go to a view, adding a history entry. */
	function go(to: ViewId) {
		void goto(`/onboarding/${to}`);
	}

	// Optimistic local flags — flip a step the instant the local signal fires,
	// before the next server poll confirms it.
	let deviceReady = $state(false);
	// Whether they have written anything yet, so the button can say "keep"
	// rather than "start" — returning to a page that has forgotten you wrote
	// eight answers is its own small insult.
	let interviewStarted = $state(false);

	function setupDone(id: string): boolean {
		return state_?.setup.find((s) => s.id === id)?.done ?? false;
	}
	function onboardingDone(id: string): boolean {
		return state_?.onboarding.find((s) => s.id === id)?.done ?? false;
	}

	const accountDone = $derived(setupDone("account"));
	const worldEnough = $derived(
		onboardingDone("first_source") ||
			onboardingDone("living_source") ||
			onboardingDone("first_phone") ||
			onboardingDone("chat_imported") ||
			deviceReady,
	);
	const narrativeReady = $derived(onboardingDone("narrative_identity_ready"));
	const narrativeGenerating = $derived(
		state_?.onboarding.find((s) => s.id === "narrative_identity_ready")?.kind === "generating",
	);

	// What the strip shows as behind you. Deliberately generous: `sources` counts
	// as passed once anything is connected, never as "complete", because more can
	// always be added and a tick would claim otherwise.
	const passed = $derived(
		[
			// The two reading screens have no server-side completion — being past
			// them in the URL order IS having passed them, which is also what makes
			// them reachable again from the strip after a refresh.
			VIEW_ORDER.indexOf(view) > VIEW_ORDER.indexOf("letter") && "letter",
			VIEW_ORDER.indexOf(view) > VIEW_ORDER.indexOf("introductions") && "names",
			accountDone && "account",
			worldEnough && "sources",
			interviewStarted && "words",
		].filter(Boolean) as StepId[],
	);

	/**
	 * Where an unresolved or unreachable URL should land.
	 *
	 * Everything past `account` needs the account, so a hand-typed
	 * `/onboarding/you` on an unlinked box must not open the reveal — the strip
	 * already refuses to offer it, and the address bar has to refuse too.
	 */
	const reachable = $derived((v: ViewId) =>
		VIEW_ORDER.indexOf(v) <= VIEW_ORDER.indexOf("account") ? true : accountDone,
	);
	const resolved = $derived<ViewId>(
		!accountDone ? "letter" : !worldEnough ? "sources" : "your-words",
	);

	/**
	 * What the strip may offer, from the SAME predicate that guards the URL.
	 *
	 * Derived rather than hand-listed for one reason: when these two drifted, the
	 * strip disabled Introductions while happily offering Account two places
	 * further along, and typing the URL worked where clicking did not.
	 */
	const open = $derived(STEPS.map((s) => s.id).filter((id) => reachable(STEP_VIEW[id])));

	async function refreshState() {
		try {
			state_ = await getSetupState();
		} catch {
			/* box briefly unreachable — keep last state */
		} finally {
			loading = false;
		}
	}

	/**
	 * Keep the address bar honest.
	 *
	 * Two jobs, both after the first state read so they never fight a box that
	 * has not answered yet: make a bare `/onboarding` name the view it is showing
	 * (so Back from step two returns to the letter rather than out of the app),
	 * and bounce a URL the person is not entitled to.
	 *
	 * `replaceState` in both cases — a correction is not somewhere they navigated
	 * to, and leaving it in history would make Back bounce off it forever.
	 */
	$effect(() => {
		if (loading || !state_) return;
		if (!isViewId(page.params.view)) {
			void goto(`/onboarding/${view}`, { replaceState: true });
		} else if (!reachable(view)) {
			void goto(`/onboarding/${resolved}`, { replaceState: true });
		}
	});

	// Cloud/onboarding cross-check for home_timezone (the box's location).
	// The box normally seeds this from its own system clock; but a datacenter box
	// reads "UTC", which is wrong. So only fall back to the onboarding browser's
	// zone when the server value is unset or UTC — a real appliance configured at
	// home keeps its server-detected zone. See docs/timezone-model.md.
	async function captureTimezone() {
		try {
			const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
			if (!tz) return;
			const p = await getProfile();
			const current = p.home_timezone;
			if (!current || current === "UTC") {
				if (current !== tz) await updateProfile({ home_timezone: tz });
			}
		} catch {
			/* non-essential */
		}
	}

	onMount(() => {
		reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
		void refreshState();
		// Best-effort: this only changes a verb on a button, and failing to read
		// it must never keep someone off the page.
		void getInterviewAnswers()
			.then((rows) => (interviewStarted = rows.some((r) => r.answer.trim().length > 0)))
			.catch(() => {});
		void captureTimezone();
		// Light poll so steps completed elsewhere (OAuth round-trip, the collector
		// daemon, the CLI) tick over here too.
		const t = setInterval(refreshState, 4000);
		return () => clearInterval(t);
	});

	async function enterApp() {
		// RECORD THE CHOICE FIRST. The app shell redirects back here while
		// onboarding is unfinished, so leaving without saying "I'm leaving on
		// purpose" would bounce straight back and read as the button being
		// broken (2026-08-13).
		//
		// Only when it is genuinely unfinished — a completed onboarding is not a
		// skipped one, and marking it skipped would lose that distinction.
		if (state_ && state_.onboarding_complete === false) {
			try {
				await skipOnboarding(true);
			} catch {
				// The gate will ask again next launch. Annoying beats trapped:
				// never let a failed write hold someone out of their own app.
			}
		}
		void goto("/");
	}

	/**
	 * Go back to a step already behind you, from the strip.
	 *
	 * BACKWARD ONLY — the header refuses to offer anything not in `passed`, and
	 * this trusts that rather than re-deriving it, because the two disagreeing
	 * is how someone ends up on the reveal with an empty box.
	 *
	 * Leaving the interview or the draft is the same motion as leaving any other
	 * screen: both save as they go, so there is nothing to confirm and nothing
	 * to lose.
	 */
	function jump(id: StepId) {
		go(STEP_VIEW[id]);
	}

	function confirmAdvanced() {
		advancedOpen = false;
		mode = "manual";
	}
</script>

{#if loading}
	<div class="flex min-h-screen items-center justify-center gap-2.5 text-sm text-foreground-muted" in:fade>
		<Icon icon="ri:loader-4-line" class="animate-spin" />
		<span>Checking your box…</span>
	</div>
{:else if !state_}
	<div class="flex min-h-screen items-center justify-center px-6">
		<div class="rounded-xl border border-error/20 bg-error-subtle p-4 text-sm text-error" in:fade>
			Couldn't reach the box. Make sure you're on the same network, then refresh.
		</div>
	</div>
{:else if mode === "manual"}
	<!-- The advanced door: just the account gate, then into the app. -->
	<div class="flex min-h-screen items-center justify-center px-6 py-16">
		<div class="w-full max-w-md">
			<p class="mb-2 font-mono text-[0.6875rem] uppercase tracking-[0.16em] text-foreground-subtle">Skipped setup</p>
			<h1 class="mb-6 font-serif text-2xl tracking-tight text-foreground">Sign in to Virtues</h1>
			<AccountGate done={accountDone} onLinked={refreshState} />
			{#if accountDone}
				<div class="mt-8" in:fade>
					<Button variant="primary" class="w-full justify-center py-2.5" onclick={enterApp}>Enter Virtues →</Button>
				</div>
			{/if}
			<button class="mt-6 font-mono text-xs text-foreground-subtle transition-colors hover:text-foreground" onclick={() => (mode = "document")}>
				← Back to guided setup
			</button>
		</div>
	</div>
{:else}
	<!-- ONE SHELL, ONE HEADER, ONE ANIMATED SLOT.
	     Every surface used to bring its own `.ob-wrap`, its own `.ob-sheet` and
	     its own copy of the strip, so the strip was a different element on every
	     screen and slid in with the content under it. Mounted once out here it
	     simply persists: `{#key view}` remounts only the leaf, so only the leaf
	     turns. -->
	<div class="ob-wrap" class:ob-still={reduced}>
		<div class="ob-sheet">
			<OnboardingHeader {step} done={passed} {open} onjump={jump} />

			{#key view}
				<div class="ob-page" class:ob-back={back}>
					{#if view === "letter"}
						<FoundersLetter onbegin={() => go("introductions")} />
					{:else if view === "introductions"}
						<!-- Introductions is the hand-off from reading to working, so
						     Continue goes wherever the box actually needs us — not
						     blindly to the next slug. -->
						<Introductions onnext={() => go(resolved)} />
					{:else if view === "interview"}
						<Interview onfinish={() => go("draft")} />
					{:else if view === "draft"}
						<DraftReview
							ondone={() => {
								interviewStarted = true;
								void refreshState();
								go("you");
							}}
						/>
					{:else if view === "account"}
						<!-- The one gate. Everything else in onboarding can be skipped,
						     put off, or half-done; this is the only screen that has to
						     end in a yes, so it is the only one with no way past. -->
						<h1 class="ob-h1">Sign in to Virtues</h1>
						<p class="ob-lede">
							Your subscription is the only part of Virtues that touches our servers — it
							handles sign-in and pays for the models your box calls on. It is built in two
							halves: one knows you pay us, the other runs your requests. They share no
							identifier, so joining them returns an empty table. Everything else stays on
							the box.
						</p>

						<div class="work">
							<AccountGate done={accountDone} onLinked={refreshState} />
						</div>

						{#if accountDone}
							<div in:fade>
								<button class="ob-btn" onclick={() => go("sources")}>
									Continue
									<Icon icon="ri:arrow-right-line" width="16" />
								</button>
							</div>
						{/if}
					{:else if view === "sources"}
						<h1 class="ob-h1">Where the record comes from</h1>
						<p class="ob-lede">
							Connect what already holds your life. Each source is read onto the box and
							stays there — nothing is sent to us. Start with one; add the rest whenever.
						</p>

						<div class="work">
							<ConnectWorld
								onConnected={refreshState}
								onDeviceReady={() => (deviceReady = true)}
							/>
						</div>

						<button class="ob-btn" onclick={() => go("your-words")}>
							{worldEnough ? "Continue" : "Skip for now"}
							<Icon icon="ri:arrow-right-line" width="16" />
						</button>
						<p class="ob-note">
							Connecting is optional, and never finished — finances, notes, fitness and the
							rest are waiting in the app.
						</p>
					{:else if view === "your-words"}
						<!-- The doorway to the interview, which is its own view. -->
						<h1 class="ob-h1">The part it can't observe</h1>
						<p class="ob-lede">
							Everything else here your box works out by watching. This is the half it
							cannot: where you have been, what you are up against, who you mean to become.
							Five questions, and nothing writes them but you.
						</p>

						<button class="ob-btn" onclick={() => go("interview")}>
							{interviewStarted ? "Keep writing" : "Start writing"}
							<Icon icon="ri:arrow-right-line" width="16" />
						</button>
						<p class="ob-note">
							It takes a while, and it saves as you go — stop anywhere and come back.
						</p>

						<button class="ob-ghost quiet-go" onclick={() => go("you")}>Not now →</button>
					{:else}
						<h1 class="ob-h1">Meet yourself</h1>
						<div class="work">
							<RevealSection
								ready={narrativeReady}
								generating={narrativeGenerating}
								{reduced}
								onEnter={enterApp}
								onConnect={() => go("sources")}
							/>
						</div>
					{/if}
				</div>
			{/key}
		</div>
	</div>

	<button class="manual-link" onclick={() => (advancedOpen = true)}>Dangerously skip onboarding →</button>
{/if}

<!-- The advanced door's confirm — the safe choice (Stay guided) is the loud one. -->
<Modal open={advancedOpen} onClose={() => (advancedOpen = false)} title="Dangerously skip onboarding?" width="sm">
	{#snippet children()}
		<div class="space-y-5 text-sm">
			<p class="leading-relaxed text-foreground-muted">
				The guided path connects your life and ends with a first draft of your narrative identity. Skipping drops you
				straight into the raw box — sources, tables, and actions, with nothing drafted for you. You'll still link your
				account (the one thing the app needs), and you can come back to setup anytime from the sidebar.
			</p>
			<div class="flex items-center justify-end gap-4">
				<button class="text-sm text-foreground-subtle transition-colors hover:text-foreground" onclick={confirmAdvanced}>
					Skip anyway
				</button>
				<Button variant="primary" class="px-5 py-2.5" onclick={() => (advancedOpen = false)}>Stay guided</Button>
			</div>
		</div>
	{/snippet}
</Modal>

<style>
	/* Four levels, not three — this file sits one deeper than it used to, under
	   `[[view]]`. svelte-check does not resolve `@reference`, so a stale path
	   here typechecks clean and 500s only the style request, which the browser
	   reports as "failed to fetch dynamically imported module" for the whole
	   page. Blank screen, no error naming this line. */
	@reference "../../../../app.css";

	/* The shell, type scale and controls come from onboarding.css — see that
	   file for why they are not here. What follows is only this route's. */

	.work {
		margin-top: 2.25rem;
	}

	.quiet-go {
		margin-top: 1.75rem;
		display: block;
	}

	.manual-link {
		position: fixed;
		top: 1.5rem;
		right: 1.75rem;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		transition: color 0.15s ease;
		z-index: var(--z-sticky);
	}
	.manual-link:hover {
		color: var(--color-foreground);
	}
</style>
