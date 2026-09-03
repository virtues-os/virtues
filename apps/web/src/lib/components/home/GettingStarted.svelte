<!--
	GettingStarted.svelte — the page the app opens on until the record is set up.

	Onboarding shrank to the founder's letter (2026-08-31, /founders-letter);
	everything it used to ask lives here, because the payoff of connecting a
	life is asynchronous — nothing kicks when a source connects, entities
	resolve on a 15-minute tick, the first narrated day lands the next morning.
	A flow the person passes through once can only promise that; a page they
	return to can show it. Design: agents/plan/getting-started-plan.md.

	A NUMBERED SEQUENCE, ALL OF IT VISIBLE. The first build rendered floating
	prose sections that individually vanished; on a half-finished box the
	survivors read as disembodied fragments with no arc (struck 2026-08-31,
	same day). Now the page is the whole list, 1 through N: done steps stay,
	checked off; the first open step carries its body inline; any open step
	can be brought forward by clicking its line. The page IS the progress
	indicator, which is why it needs no other framing prose.

	WHILE ANY STEP IS OPEN, THIS IS THE PAGE. HomeView renders nothing else —
	no subtitle, no day stepper, no deck of silent tracks — until every step
	is done or waved away; only then does Home exist. The `phase` binding
	tells HomeView which page this is ("loading" holds until both ends have
	answered once, so a first-run box never flashes Home's furniture).

	SKIPPED MEANS GONE FROM THE LIST'S DEMANDS, NOT FROM THE PRODUCT. A
	skipped step counts as settled and shows as "skipped"; the same asks stay
	findable where they permanently live (Settings → Sources, the interview
	in the sidebar). Dismissals persist on the profile so the page reads the
	same on every glass. The hidden door at the foot skips everything at once.
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
	import StepPlate from "./StepPlate.svelte";

	// Mirrors narrative_draft::INTERVIEW_CHAT_ID (and ChatView's copy).
	const INTERVIEW_CHAT_ID = "chat_narrative_interview";

	/** What the hidden door skips at once — every step that can be. */
	const ALL_DISMISSIBLE = ["introductions", "connect", "interview", "first_day", "further"];

	let {
		phase = $bindable("loading"),
	}: {
		/** Which page this is — HomeView reads this, never sets it. */
		phase?: "loading" | "focus" | "settled";
	} = $props();

	let profile = $state<Profile | null>(null);
	let census = $state<Census | null>(null);
	let dismissed = $state<string[]>([]);
	/** Set even on failure — "couldn't read the profile" must not hold the
	 *  whole of Home in the loading state forever. */
	let profileSettled = $state(false);
	/** The Mac collector finishing fires before the next setup-state poll. */
	let deviceReady = $state(false);

	const store = setupStateStore;

	async function loadProfile() {
		try {
			profile = await getProfile();
			dismissed = profile.getting_started_dismissed ?? [];
		} catch {
			/* box briefly unreachable — steps that need the profile wait */
		} finally {
			profileSettled = true;
		}
	}

	async function loadCensus() {
		try {
			census = await getCensus();
		} catch {
			/* same: the first-day step simply doesn't advance this tick */
		}
	}

	function isDismissed(id: string): boolean {
		return dismissed.includes(id);
	}

	/**
	 * Optimistic, but honest: revert on a failed write, so the page never
	 * claims a skip the box doesn't hold — a step that reopens next launch
	 * after being waved away reads as a nag with amnesia.
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
	const firstDay = $derived(census?.first_day ?? null);
	/** "⚠ needs Full Disk Access" (or the denied capability's name) when a
	 *  collector is running with a permission hole; null when all is well. */
	const degradedNote = $derived.by(() => {
		const d = store.degraded[0];
		if (!d) return null;
		const cap = d.denied[0] === "full_disk_access" ? "Full Disk Access" : (d.denied[0] ?? "a permission");
		return `⚠ needs ${cap}`;
	});

	// ---- the steps, in walking order ----
	// `done` means settled: answered, arrived, or skipped. The sign-in step
	// exists only where an account is the box's business at all — linked
	// already (shown done), or an appliance still waiting on one. A DIY box
	// that satisfies setup without an account never sees the line.
	//
	// `state` is the right-hand column, and it speaks the CLI's vocabulary
	// (✓ · — see agents/build + cli/ui.rs): the terminal, the box, and this
	// page report in one voice. It also tells TIME when it can — a pending
	// first day says when it arrives, because a status column that only ever
	// says "done" is decoration.
	type StepId = "letter" | "introductions" | "connect" | "signin" | "interview" | "first_day" | "further";
	const steps = $derived.by(() => {
		// One vocabulary: a settled step is ✓, full stop. The column spends
		// words only when they inform — "· skipped", "· underway", "⚠ needs…",
		// "· tomorrow morning" — never to restate the mark.
		const settled = (id: string, earned: boolean) =>
			earned ? "✓" : isDismissed(id) ? "· skipped" : "";
		const rows: { id: StepId; title: string; done: boolean; state: string }[] = [
			{ id: "letter", title: "The founder's letter", done: true, state: "✓" },
			{
				id: "introductions",
				title: "Introductions",
				done: !!profile?.preferred_name || isDismissed("introductions"),
				state: settled("introductions", !!profile?.preferred_name),
			},
			{
				id: "connect",
				title: "Connect your world",
				done: worldEnough || isDismissed("connect"),
				// A collector running with a denied permission outranks the
				// check: a ✓ over a permission hole is how the three-day
				// iMessage outage happened.
				state: degradedNote ?? settled("connect", worldEnough),
			},
		];
		if (accountDone || !store.accountSatisfied) {
			rows.push({ id: "signin", title: "Connect your Virtues account", done: accountDone, state: accountDone ? "✓" : "" });
		}
		rows.push(
			{
				id: "interview",
				title: "In your own words",
				done: store.done("narrative_identity_ready") || isDismissed("interview"),
				// The days between a first answer and "write it up" are the
				// normal case — the column says so instead of nagging "Start".
				state:
					settled("interview", store.done("narrative_identity_ready")) ||
					(store.interviewStarted ? "· underway" : ""),
			},
			{
				id: "first_day",
				title: "Your first day, written up",
				done: firstDay !== null || isDismissed("first_day"),
				// Pending, with sources flowing: the column says when. Nothing
				// connected yet, no promise — the overnight write needs a day
				// of record to write about.
				state:
					firstDay !== null ? "" : isDismissed("first_day") ? "· skipped" : worldEnough ? "· tomorrow morning" : "",
			},
			{
				id: "further",
				title: "Go further",
				done: isDismissed("further"),
				// A wave-away is a skip, not an achievement — same vocabulary
				// as every other dismissed row.
				state: isDismissed("further") ? "· skipped" : "",
			},
		);
		return rows;
	});

	/** A step the person clicked forward, overriding the default. Done steps
	 *  are choosable too — a sequence you cannot walk back through reads as a
	 *  ratchet, and introductions or sources are worth reopening. */
	let chosen = $state<StepId | null>(null);
	/** The open step: chosen, else the first not-done one. */
	const active = $derived(chosen ?? steps.find((s) => !s.done)?.id ?? null);

	const anything = $derived(steps.some((s) => !s.done));
	/** Both ends have answered once. Rendering waits for this too — before
	 *  the profile lands, introductions counts as open and its card flashes
	 *  in only to fold away a beat later. */
	const ready = $derived(store.loaded && profileSettled);
	$effect(() => {
		phase = !ready ? "loading" : anything ? "focus" : "settled";
	});

	onMount(() => {
		void loadProfile();
		void loadCensus();
		// The first day lands on the box's own clock (narration runs at the
		// maintenance hour, once a day) — a five-minute beat is generous, and
		// the old 60s one was ~27 count(*) queries a minute for up to a day.
		const t = setInterval(() => {
			if (document.hidden || firstDay !== null) return;
			void loadCensus();
		}, 300_000);
		return () => clearInterval(t);
	});

	function openDay(date: string) {
		windowShellStore.openTabFromRoute(`/day/day_${date}`, { label: "Your first day" });
	}
	function openInterview() {
		windowShellStore.openTabFromRoute(`/chat/${INTERVIEW_CHAT_ID}`, { label: "In your own words" });
	}
	function openApplets() {
		windowShellStore.openTabFromRoute("/applets");
	}

	/** "YYYY-MM-DD" → "August 30". Split-and-construct, not `new Date(str)` —
	 *  the string form parses as UTC and shifts a day west of Greenwich. */
	function prettyDay(ymd: string): string {
		const [y, m, d] = ymd.split("-").map(Number);
		return new Date(y, m - 1, d).toLocaleDateString(undefined, { month: "long", day: "numeric" });
	}

	// The hidden door: first click names it, second click acts. Devs and
	// power users try the one unexplained icon; everyone else shouldn't read
	// an invitation to leave.
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

{#if ready && anything}
	<div class="gs folio" in:fade={{ duration: 200 }}>
		<button
			class="door"
			class:expanded={doorExpanded}
			onclick={doorClick}
			onblur={() => (doorExpanded = false)}
			aria-label="Skip getting started"
		>
			{#if doorExpanded}
				<span in:fade={{ duration: 120 }}>
					{accountDone || store.accountSatisfied ? "Skip getting started →" : "Skip the rest — sign-in remains →"}
				</span>
			{:else}
				<Icon icon="ri:door-open-line" width="14" />
			{/if}
		</button>

		<div class="mark serif" aria-hidden="true">∴</div>

		<ol class="steps">
			{#each steps as s, i (s.id)}
				<li class="step" class:done={s.done} class:active={s.id === active}>
					<div class="line">
						<span class="n mono">{i + 1}</span>
						{#if s.id !== active}
							<button class="t" type="button" onclick={() => (chosen = s.id)}>{s.title}</button>
						{:else}
							<span class="t">{s.title}</span>
						{/if}
						{#if s.id === "first_day" && firstDay !== null}
							<button class="state link" type="button" onclick={() => openDay(firstDay)}>
								Read {prettyDay(firstDay)} <span class="arw">→</span>
							</button>
						{:else if s.state}
							<span class="state mono">{s.state}</span>
						{/if}
					</div>

					{#if s.id === active}
						<div class="body" in:fade={{ duration: 150 }}>
							{#if s.id === "letter"}
								<a class="link" href="/founders-letter">Read it again <span class="arw">→</span></a>
							{:else if s.id === "introductions"}
								<p class="lede">The few things it cannot learn by reading.</p>
								<IntroductionsCard
									ondone={() => {
										void loadProfile();
										void dismiss("introductions");
										chosen = null;
									}}
									ondismiss={() => {
										void dismiss("introductions");
										chosen = null;
									}}
								/>
							{:else if s.id === "connect"}
								<p class="lede">What already holds your life — your server reads it from here on.</p>
								<ConnectWorld
									onConnected={() => void store.check()}
									onDeviceReady={() => (deviceReady = true)}
									next="/"
								/>
								{#if !s.done}
									<button class="skip" type="button" onclick={() => void dismiss("connect")}>
										Skip — sources stay available in Settings
									</button>
								{/if}
							{:else if s.id === "signin"}
								<p class="lede">
									The account is how your server reaches anything outside itself: the
									models it writes with, plus the maps, photos, bank links and calendar
									connections. One subscription, metered per request and never kept —
									bringing your own AI key later moves the model calls and leaves the
									rest here.
								</p>
								<div class="work">
									<AccountGate done={accountDone} onLinked={() => void store.check()} />
								</div>
							{:else if s.id === "interview"}
								<p class="lede">
									A conversation about your life: its chapters, what makes you unlike
									others, what you believe, what a good day is. About twenty minutes,
									one question at a time. Skip anything, stop anywhere.
								</p>
								<div class="row">
									<button class="link" type="button" onclick={openInterview}>
										{store.interviewStarted ? "Pick up where you left off" : "Start the interview"} <span class="arw">→</span>
									</button>
									{#if !s.done}
										<button class="skip" type="button" onclick={() => void dismiss("interview")}>Skip</button>
									{/if}
								</div>
							{:else if s.id === "first_day"}
								<p class="lede">
									Overnight, your server writes yesterday down — where you went, who you
									spoke with, what the day was. The first page arrives tomorrow morning,
									and every day after writes itself.
								</p>
								{#if firstDay !== null}
									<button class="link" type="button" onclick={() => openDay(firstDay)}>
										Read {prettyDay(firstDay)} <span class="arw">→</span>
									</button>
								{:else if !s.done}
									<button class="skip" type="button" onclick={() => void dismiss("first_day")}>
										Don't wait for it here
									</button>
								{/if}
							{:else if s.id === "further"}
								<ul class="plain">
									<li>
										<button class="link" type="button" onclick={openApplets}>
											Create your first applet <span class="arw">→</span>
										</button>
										<span class="note">a small program your box runs for you</span>
									</li>
									<li>
										<a class="link" href="https://virtues.com/docs" target="_blank" rel="noreferrer">
											Read the manual <span class="arw">→</span>
										</a>
										<span class="note">how all of this works</span>
									</li>
								</ul>
								{#if !s.done}
									<button class="skip" type="button" onclick={() => void dismiss("further")}>
										All set
									</button>
								{/if}
							{/if}
						</div>
					{/if}
				</li>
			{/each}
		</ol>

		{#if census && census.total > 0 && census.lines.length > 0}
			<!-- Proof of life, one sentence: the census is already on the wire
			     for the first-day date; this reads the rest of it. No card, no
			     counter animation — the server stating a fact. -->
			<p class="census">
				So far the record holds {census.lines[0].count.toLocaleString()}
				{census.lines[0].label}{#if census.lines[1]}&nbsp;and {census.lines[1].count.toLocaleString()}
					{census.lines[1].label}{/if}{#if census.earliest};
					the oldest trace is from {new Date(census.earliest).toLocaleDateString(undefined, { month: "long", year: "numeric" })}{/if}.
			</p>
		{/if}

		<!-- The folio's right page: the active step's plate — a figure for
		     what this step IS, the way an atlas pairs the photograph with the
		     hand-drawn diagram. Wide screens only; the walk never depends on
		     it. -->
		<aside class="plate-rail">
			<StepPlate step={active ?? "settled"} {census} />
		</aside>
	</div>
{/if}

<style>
	.mono { font-family: var(--font-mono); font-variant-numeric: tabular-nums; }

	/* Full width, so the door can sit at the page's right edge; the sequence
	   itself keeps a reading measure. */
	.gs { position: relative; padding: clamp(4px, 1.5vh, 16px) 0 8px; }

	/* The folio: the walk is the left page, the plate the right. The plate
	   is companionship, not chrome — below 1080px it simply isn't, and the
	   walk reads exactly as before. */
	.folio {
		display: grid;
		grid-template-columns: minmax(0, 640px) 250px;
		column-gap: clamp(32px, 6vw, 88px);
		align-items: start;
	}

	.plate-rail {
		grid-column: 2;
		grid-row: 1 / span 2;
		position: sticky;
		top: 24px;
		margin-top: 44px;
	}

	@media (max-width: 1080px) {
		.folio { display: block; }
		.plate-rail { display: none; }
	}

	.steps { list-style: none; margin: 0; padding: 0; max-width: 640px; }

	.steps { --gutter: 30px; }

	.mark {
		font-family: var(--font-serif);
		font-size: 15px;
		color: var(--color-foreground-subtle);
		padding: 0 0 6px 1px;
	}

	.census {
		max-width: 640px;
		margin: 14px 0 0;
		padding-left: var(--gutter);
		font-family: var(--font-serif);
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.step { border-top: 1px solid var(--color-border-subtle); }
	.step:first-child { border-top: 0; }

	.line { display: flex; align-items: baseline; gap: 16px; padding: 16px 0; }
	.step.active .line { padding-bottom: 6px; }

	/* Chapter numerals, not row ids: the titles' own serif, oldstyle figures,
	   one size down. The open step's numeral is the "you are here" — full
	   ink, and the only mark the active row gets. */
	.n {
		flex: none; width: var(--gutter);
		font-family: var(--font-serif); font-size: 15px;
		font-variant-numeric: oldstyle-nums;
		color: var(--color-foreground-subtle);
		transition: color 0.15s ease;
	}
	.step.active .n { color: #9a2b2e; }

	.t {
		font-family: var(--font-serif); font-size: 19px; font-weight: 400;
		line-height: 1.35; color: var(--color-foreground);
		background: none; border: 0; padding: 0; text-align: left; min-width: 0;
		transition: color 0.15s ease;
	}
	button.t { cursor: pointer; }
	/* The past recedes, the present is full ink, the future waits in
	   between — three weights, read at a glance. */
	.step.done .t { color: var(--color-foreground-subtle); }
	.step.done .n { color: color-mix(in srgb, var(--color-foreground-subtle) 55%, transparent); }
	.step.active .t { color: var(--color-foreground); }

	/* Hover stays in the book's voice: ink, not link-blue. The whole line
	   answers, so a row reads as pressable before the cursor finds the
	   title's exact glyphs. */
	.line:hover .n { color: var(--color-foreground-muted); }
	.step.active .line:hover .n { color: var(--color-foreground); }
	.line:hover button.t { color: var(--color-foreground); }

	.state { margin-left: auto; flex: none; font-size: 11.5px; color: var(--color-foreground-subtle); white-space: nowrap; }

	/* The open step's body sits in the title's column, not under the number,
	   on the same left edge as the ledes. */
	.body { padding: 0 0 clamp(24px, 3.5vh, 40px) calc(var(--gutter) + 16px); }

	.lede {
		font-family: var(--font-sans); font-size: 14px; line-height: 1.55;
		color: var(--color-foreground-muted); margin: 0 0 14px; max-width: 56ch;
	}

	.work { margin-top: 4px; }

	.plain { list-style: none; margin: 0 0 6px; padding: 0; }
	.plain li { display: flex; align-items: baseline; gap: 10px; padding: 4px 0; }
	.plain .note { color: var(--color-foreground-subtle); font-size: 12.5px; min-width: 0; }

	.row { display: flex; align-items: baseline; gap: 16px; }

	.link {
		font-family: var(--font-sans); font-size: 13.5px; font-weight: 500;
		color: var(--color-primary); background: none; border: 0; padding: 0;
		cursor: pointer; text-align: left; text-decoration: none;
	}
	.link:hover { text-decoration: underline; text-underline-offset: 3px; }
	.link .arw { opacity: 0.7; }
	.state.link { margin-left: auto; font-size: 12.5px; }

	.skip {
		display: block; margin-top: 12px;
		font-family: var(--font-sans); font-size: 12.5px;
		color: var(--color-foreground-subtle); background: none; border: 0;
		padding: 0; cursor: pointer;
	}
	.skip:hover { color: var(--color-foreground); }
	.row .skip { margin-top: 0; }

	/* The hidden door — same coat as the old dangerously-skip, top right and
	   rightmost on the page, clear of the reading column. */
	.door {
		position: absolute; right: 0; top: clamp(4px, 1.5vh, 16px);
		display: flex; align-items: center;
		font-family: var(--font-mono); font-size: 11px;
		color: var(--color-foreground-subtle); background: none; border: 0;
		padding: 2px; cursor: pointer; opacity: 0.45;
		transition: color 0.15s ease, opacity 0.15s ease;
	}
	.door:hover, .door.expanded { color: var(--color-foreground); opacity: 1; }
</style>
