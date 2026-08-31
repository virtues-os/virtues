<!--
	GettingStarted.svelte — Home's first-run state, as sections that retire.

	Onboarding shrank to the founder's letter (2026-08-31); everything it used
	to ask now lives here, because the payoff of connecting a life is
	asynchronous — nothing kicks when a source connects, entities resolve on a
	15-minute tick, the first narrated day lands the next morning. A flow the
	person passes through once can only promise that; a page they return to
	can show it. Design: agents/plan/getting-started-plan.md.

	ONE RULE, NOT PER-SECTION JUDGMENT. Sections that ask (introductions,
	connect, the interview, enrichment) retire when answered or when waved
	away; sections that show (the schedule, the first day page) retire on
	their own when their promise lands. There is no completion flag and no
	mode switch — when every section here has retired, this component renders
	nothing, and what remains is Home.

	DISMISSED MEANS GONE, NOT COLLAPSED. No residue rows: the same asks stay
	findable where they permanently live (Settings → Sources, the interview in
	the sidebar). Dismissals persist on the profile so the page sheds
	identically on every glass. The one escape hatch is the hidden door at the
	block's foot — the same tucked-away gesture onboarding's skip used to be.

	TWO DRESSES, ONE COMPONENT. While the askers are open (sign-in,
	introductions, connect) this IS the page — HomeView renders nothing else,
	because a getting-started block sharing a screen with a nine-track day
	chart of silence is a mess wearing two costumes. The `phase` binding tells
	HomeView which dress is on: "focus" until the askers settle, "settled"
	once only the quiet residuals (schedule, first day, interview, enrichment)
	remain, "loading" while neither end has answered yet.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { fade } from "svelte/transition";
	import Icon from "$lib/components/Icon.svelte";
	import { getProfile, updateProfile, getCensus, type Profile, type Census } from "$lib/api/client";
	import { setupStateStore } from "$lib/stores/setupState.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import AccountGate from "$lib/components/onboarding/document/AccountGate.svelte";
	import ConnectWorld from "$lib/components/onboarding/document/ConnectWorld.svelte";
	import IntroductionsCard from "./IntroductionsCard.svelte";

	// Mirrors narrative_draft::INTERVIEW_CHAT_ID (and ChatView's copy).
	const INTERVIEW_CHAT_ID = "chat_narrative_interview";

	/** Every id the hidden door dismisses at once. `first_day` is a shower,
	 *  but the door means "I want none of this" — so it goes too. */
	const ALL_DISMISSIBLE = ["introductions", "connect", "interview", "applet", "learn", "first_day"];

	let {
		phase = $bindable("loading"),
	}: {
		/** Which dress the page wears — HomeView reads this, never sets it. */
		phase?: "loading" | "focus" | "settled";
	} = $props();

	let profile = $state<Profile | null>(null);
	let census = $state<Census | null>(null);
	let dismissed = $state<string[]>([]);
	/** Set even on failure — "couldn't read the profile" must not hold the
	 *  whole of Home in the loading dress forever. */
	let profileSettled = $state(false);
	/** The Mac collector finishing fires before the next setup-state poll. */
	let deviceReady = $state(false);

	const store = setupStateStore;

	async function loadProfile() {
		try {
			profile = await getProfile();
			dismissed = profile.getting_started_dismissed ?? [];
		} catch {
			/* box briefly unreachable — sections that need the profile wait */
		} finally {
			profileSettled = true;
		}
	}

	async function loadCensus() {
		try {
			census = await getCensus();
		} catch {
			/* same: the schedule simply doesn't advance this tick */
		}
	}

	function isDismissed(id: string): boolean {
		return dismissed.includes(id);
	}

	/**
	 * Optimistic, but honest: revert on a failed write, so the page never
	 * claims a dismissal the box doesn't hold — a section that reappears next
	 * launch after being waved away reads as a nag with amnesia.
	 */
	async function dismiss(...ids: string[]) {
		const before = dismissed;
		dismissed = [...new Set([...dismissed, ...ids])];
		try {
			await updateProfile({ getting_started_dismissed: dismissed });
		} catch {
			dismissed = before;
		}
	}

	// ---- the signals ----
	const accountDone = $derived(store.setup.find((s) => s.id === "account")?.done ?? false);
	const worldEnough = $derived(store.worldEnough || deviceReady);
	const peopleLanded = $derived(
		(census?.lines ?? []).some((l) => l.id === "people" || l.id === "places"),
	);
	const firstDay = $derived(census?.first_day ?? null);

	// ---- the sections, each with its own "nothing left to say" ----
	const showSignin = $derived(store.loaded && !store.accountSatisfied);
	const showIntro = $derived(
		profile !== null && !isDismissed("introductions") && !profile.preferred_name,
	);
	const showConnect = $derived(store.loaded && !isDismissed("connect") && !worldEnough);
	/** Promises still out. Only speaks once something is connected — a box
	 *  with nothing flowing has nothing arriving. */
	const scheduleLines = $derived.by(() => {
		if (!worldEnough || census === null) return [] as string[];
		const lines: string[] = [];
		if (!peopleLanded)
			lines.push("People and places, worked out from the record — within the quarter hour.");
		if (!firstDay)
			lines.push("Your first day, written up — tomorrow morning. Every day after writes itself overnight.");
		return lines;
	});
	const showFirstDay = $derived(firstDay !== null && !isDismissed("first_day"));
	const showInterview = $derived(
		store.loaded && !isDismissed("interview") && !store.done("narrative_identity_ready"),
	);
	const enrichment = $derived(
		[
			{ id: "applet", label: "Create your first applet", note: "a small program your box runs for you", route: "/applets" },
			{ id: "learn", label: "Read the manual", note: "how all of this works", href: "https://virtues.com/docs" },
		].filter((r) => !isDismissed(r.id)),
	);

	const anything = $derived(
		showSignin ||
			showIntro ||
			showConnect ||
			scheduleLines.length > 0 ||
			showFirstDay ||
			showInterview ||
			enrichment.length > 0,
	);

	// The page's dress. Open askers own the whole screen; quiet residuals
	// share it with Home. Loading holds until both ends have answered once,
	// so a first-run box never flashes Home's furniture before the focus
	// dress lands.
	const focus = $derived(showSignin || showIntro || showConnect);
	$effect(() => {
		phase = !store.loaded || !profileSettled ? "loading" : focus ? "focus" : "settled";
	});

	onMount(() => {
		void loadProfile();
		void loadCensus();
		// The schedule's promises land on the box's own clock (entity resolver
		// ~15 min, narration overnight); a 60s beat is plenty, and a hidden tab
		// asks for nothing.
		const t = setInterval(() => {
			if (document.hidden) return;
			if (scheduleLines.length === 0 && (firstDay !== null || !worldEnough)) return;
			void loadCensus();
		}, 60_000);
		return () => clearInterval(t);
	});

	function openDay(date: string) {
		windowShellStore.openTabFromRoute(`/day/day_${date}`, { label: "Your first day" });
		void dismiss("first_day");
	}
	function openInterview() {
		windowShellStore.openTabFromRoute(`/chat/${INTERVIEW_CHAT_ID}`, { label: "Interview" });
	}
	function openRoute(route: string) {
		windowShellStore.openTabFromRoute(route);
	}

	/** "YYYY-MM-DD" → "August 30". Split-and-construct, not `new Date(str)` —
	 *  the string form parses as UTC and shifts a day west of Greenwich. */
	function prettyDay(ymd: string): string {
		const [y, m, d] = ymd.split("-").map(Number);
		return new Date(y, m - 1, d).toLocaleDateString(undefined, { month: "long", day: "numeric" });
	}

	// The hidden door: first click names it, second click acts. Same gesture
	// onboarding's dangerously-skip used — devs and power users try the one
	// unexplained icon; everyone else shouldn't read an invitation to leave.
	let doorExpanded = $state(false);
	function doorClick() {
		if (doorExpanded) {
			void dismiss(...ALL_DISMISSIBLE);
			doorExpanded = false;
		} else {
			doorExpanded = true;
		}
	}
</script>

{#if anything}
	<div class="gs" class:focus in:fade={{ duration: 200 }}>
		{#if focus}
			<!-- The Page heading already says "Getting started", so no kicker —
			     that would stutter. The standfirst is the page's one line of
			     framing; it leaves with the focus dress. -->
			<p class="stand">A few things to set up, then the record takes over.</p>
		{:else}
			<h2 class="kicker">getting started</h2>
		{/if}

		{#if showSignin}
			<section class="sec">
				<h3 class="q">Sign in to Virtues.</h3>
				<p class="lede">
					The models your box writes with are what the subscription pays for. Signing in is the
					only part of Virtues that touches our servers — everything written stays here.
				</p>
				<div class="work">
					<AccountGate done={accountDone} onLinked={() => void store.check()} />
				</div>
			</section>
		{/if}

		{#if showIntro}
			<section class="sec">
				<h3 class="q">Introductions — the few things it cannot learn by reading.</h3>
				<div class="work">
					<IntroductionsCard
						ondone={() => {
							void loadProfile();
							void dismiss("introductions");
						}}
						ondismiss={() => void dismiss("introductions")}
					/>
				</div>
			</section>
		{/if}

		{#if showConnect}
			<section class="sec">
				<h3 class="q">Connect what already holds your life.</h3>
				<div class="work">
					<ConnectWorld
						onConnected={() => void store.check()}
						onDeviceReady={() => (deviceReady = true)}
						next="/"
					/>
				</div>
				<button class="skip" type="button" onclick={() => void dismiss("connect")}>
					Not now — I'll find this in Settings later
				</button>
			</section>
		{/if}

		{#if scheduleLines.length}
			<section class="sec">
				<h3 class="q">Your box is reading.</h3>
				<ul class="plain">
					{#each scheduleLines as line (line)}
						<li>{line}</li>
					{/each}
				</ul>
			</section>
		{/if}

		{#if showFirstDay && firstDay}
			<section class="sec">
				<h3 class="q">Your first day is written up.</h3>
				<button class="link" type="button" onclick={() => openDay(firstDay)}>
					Read {prettyDay(firstDay)} <span class="arw">→</span>
				</button>
			</section>
		{/if}

		{#if showInterview}
			<section class="sec">
				<h3 class="q">Your first conversation is waiting.</h3>
				<p class="lede">
					It will ask about your life — the chapters, the people, what you believe. Answer
					plainly, skip anything, come back whenever; it becomes a document in your own words,
					and it is never finished.
				</p>
				<div class="row">
					<button class="link" type="button" onclick={openInterview}>
						Open the conversation <span class="arw">→</span>
					</button>
					<button class="skip" type="button" onclick={() => void dismiss("interview")}>Not now</button>
				</div>
			</section>
		{/if}

		{#if enrichment.length}
			<section class="sec">
				<ul class="plain rows">
					{#each enrichment as r (r.id)}
						<li>
							{#if r.href}
								<a class="link" href={r.href} target="_blank" rel="noreferrer">
									{r.label} <span class="arw">→</span>
								</a>
							{:else if r.route}
								<button class="link" type="button" onclick={() => openRoute(r.route)}>
									{r.label} <span class="arw">→</span>
								</button>
							{/if}
							<span class="note">{r.note}</span>
							<button
								class="x"
								type="button"
								onclick={() => void dismiss(r.id)}
								aria-label={`Dismiss "${r.label}"`}
							>
								<Icon icon="ri:close-line" width="14" />
							</button>
						</li>
					{/each}
				</ul>
			</section>
		{/if}

		<button
			class="door"
			class:expanded={doorExpanded}
			onclick={doorClick}
			onblur={() => (doorExpanded = false)}
			aria-label="Dismiss getting started"
		>
			{#if doorExpanded}
				<span in:fade={{ duration: 120 }}>Dismiss getting started →</span>
			{:else}
				<Icon icon="ri:door-open-line" width="14" />
			{/if}
		</button>
	</div>
{/if}

<style>
	.gs { position: relative; max-width: 640px; padding-bottom: 8px; }

	.kicker {
		font-family: var(--font-mono); font-size: 10.5px; letter-spacing: 0.04em;
		color: var(--color-foreground-subtle); margin: 0 0 18px; font-weight: 400;
	}

	.sec { margin-bottom: clamp(28px, 4vh, 44px); }

	/* The focus dress: this is the whole page, so the type and the air both
	   grow — a screen with three things on it should not be set like a margin
	   note. */
	.stand {
		font-family: var(--font-serif); font-size: 21px; line-height: 1.45;
		color: var(--color-foreground); margin: 0 0 clamp(36px, 6vh, 64px); max-width: 30ch;
	}
	.gs.focus { padding-top: clamp(8px, 2vh, 24px); }
	.gs.focus .sec { margin-bottom: clamp(40px, 7vh, 72px); }
	.gs.focus .q { font-size: 20px; margin-bottom: 10px; }
	.gs.focus .lede { font-size: 14px; }

	/* The section's own line, in the voice the rest of Home speaks. */
	.q {
		font-family: var(--font-serif); font-size: 18px; font-weight: 400;
		line-height: 1.4; color: var(--color-foreground); margin: 0 0 8px;
	}
	.lede {
		font-family: var(--font-sans); font-size: 13.5px; line-height: 1.55;
		color: var(--color-foreground-muted); margin: 0 0 4px; max-width: 56ch;
	}

	.work { margin-top: 14px; }

	.plain { list-style: none; margin: 4px 0 0; padding: 0; }
	.plain li {
		font-family: var(--font-sans); font-size: 13.5px; line-height: 1.55;
		color: var(--color-foreground-muted); padding: 3px 0;
	}

	.rows li { display: flex; align-items: baseline; gap: 10px; }
	.rows .note { color: var(--color-foreground-subtle); font-size: 12.5px; min-width: 0; }
	.rows .x {
		margin-left: auto; flex: none; display: flex; align-items: center;
		background: none; border: 0; padding: 3px; border-radius: 5px;
		color: var(--color-foreground-subtle); cursor: pointer; opacity: 0;
		transition: opacity 0.15s ease, color 0.15s ease;
	}
	.rows li:hover .x, .rows .x:focus-visible { opacity: 1; }
	.rows .x:hover { color: var(--color-foreground); }

	.row { display: flex; align-items: baseline; gap: 16px; margin-top: 6px; }

	.link {
		font-family: var(--font-sans); font-size: 13.5px; font-weight: 500;
		color: var(--color-primary); background: none; border: 0; padding: 0;
		cursor: pointer; text-align: left; text-decoration: none;
	}
	.link:hover { text-decoration: underline; text-underline-offset: 3px; }
	.link .arw { opacity: 0.7; }

	.skip {
		display: block; margin-top: 12px;
		font-family: var(--font-sans); font-size: 12.5px;
		color: var(--color-foreground-subtle); background: none; border: 0;
		padding: 0; cursor: pointer;
	}
	.skip:hover { color: var(--color-foreground); }
	.row .skip { margin-top: 0; }

	/* The hidden door — same coat as onboarding's old skip. */
	.door {
		position: absolute; right: 0; bottom: -6px;
		display: flex; align-items: center;
		font-family: var(--font-mono); font-size: 11px;
		color: var(--color-foreground-subtle); background: none; border: 0;
		padding: 2px; cursor: pointer; opacity: 0.45;
		transition: color 0.15s ease, opacity 0.15s ease;
	}
	.door:hover, .door.expanded { color: var(--color-foreground); opacity: 1; }
</style>
