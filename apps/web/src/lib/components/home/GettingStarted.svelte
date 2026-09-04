<!--
	GettingStarted.svelte — the page the app opens on until the record is set up.

	Onboarding shrank to the founder's letter (2026-08-31, /founders-letter);
	everything it used to ask lives here, because the payoff of connecting a
	life is asynchronous — nothing kicks when a source connects, entities
	resolve on a 15-minute tick, the first narrated day lands the next morning.
	A flow the person passes through once can only promise that; a page they
	return to can show it.

	A SPREAD (2026-09-04). A book opens on a spread: the frontispiece on the
	left, the title page on the right. The frontispiece is a painting for the
	step at hand with one line from the bank (agents/build/voice.md) and the
	record's numbers on it in white — the record introduced as a work, not a
	stats row. The facing page is the work: a stepper — every step in walking
	order with its state in the rail, the current one open IN PLACE — the
	introductions card, the sources, the account gate are the step's body,
	not a page you are sent to — and done ones reopenable from their row
	(the letter is always one click away; it is a page of its own).

	Four earlier shapes were struck the same day: an accordion whose rows
	unfolded into forms (nobody could tell done from next), a flat list of
	verbs (sent you away without a word), a card floating over a banner
	(a dashboard in a costume), and a book page with shoulder notes (read as
	a dated form). What survived every review was: a picture, a real button,
	the letter within reach, and few words.

	WHILE ANY STEP IS OPEN, THIS IS THE PAGE. HomeView renders nothing else
	until every step is done or waved away; only then does Home exist. The
	`phase` binding tells HomeView which page this is ("loading" holds until
	both ends have answered once, so a first-run box never flashes Home).

	MOUNTED ONCE. HomeView keeps this component outside its Page shell and
	never re-creates it: the first spread build switched Page's props on the
	phase, Page moved its children between branches, this component was
	re-created with phase "loading", and the two chased each other — twelve
	instances a second, each with a census request in flight, the visible
	one always the newest. If HomeView ever changes its structure around
	this component on `phase`, that loop comes back.

	SKIPPED MEANS GONE FROM THE LIST'S DEMANDS, NOT FROM THE PRODUCT. A
	skipped step counts as settled; the same asks stay findable where they
	permanently live. Dismissals persist on the profile so the page reads the
	same on every glass. The hidden door at the foot skips everything at once.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { fade } from "svelte/transition";
	import { goto } from "$app/navigation";
	import Icon from "$lib/components/Icon.svelte";
	import AccountGate from "$lib/components/onboarding/document/AccountGate.svelte";
	import ConnectWorld from "$lib/components/onboarding/document/ConnectWorld.svelte";
	import IntroductionsCard from "./IntroductionsCard.svelte";
	import { getProfile, updateProfile, getCensus, type Profile, type Census } from "$lib/api/client";
	import { setupStateStore } from "$lib/stores/setupState.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import Frontispiece from "./Frontispiece.svelte";

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

	type StepId = "letter" | "introductions" | "connect" | "signin" | "interview" | "first_day" | "further";

	let profile = $state<Profile | null>(null);
	let census = $state<Census | null>(null);
	let dismissed = $state<string[]>([]);
	/** Set even on failure — "couldn't read the profile" must not hold the
	 *  whole of Home in the loading state forever. */
	let profileSettled = $state(false);
	/** The Mac collector finishing fires before the next setup-state poll. */
	let deviceReady = $state(false);
	/** A step the person opened by hand — a done one to revisit, or one
	 *  ahead of the walk. Null means "the first not-done step". */
	let chosen = $state<StepId | null>(null);

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

	function open(route: string, label?: string) {
		windowShellStore.openTabFromRoute(route, label ? { label } : undefined);
	}

	// ---- the signals ----
	const accountDone = $derived(store.setup.find((s) => s.id === "account")?.done ?? false);
	const worldEnough = $derived(store.worldEnough || deviceReady);
	const firstDay = $derived(census?.first_day ?? null);
	/** "needs Full Disk Access" (or the denied capability's name) when a
	 *  collector is running with a permission hole; null when all is well. */
	const degradedNote = $derived.by(() => {
		const d = store.degraded[0];
		if (!d) return null;
		const cap = d.denied[0] === "full_disk_access" ? "Full Disk Access" : (d.denied[0] ?? "a permission");
		return `Needs ${cap}`;
	});

	// ---- the steps, in walking order ----
	// `done` means settled: answered, arrived, or skipped. The sign-in step
	// exists only where an account is the box's business at all — linked
	// already (shown done), or an appliance still waiting on one. A DIY box
	// that satisfies setup without an account never sees the line.
	interface Step {
		id: StepId;
		title: string;
		done: boolean;
		/** The panel's one paragraph: what this is, in the person's words. */
		what: string;
		/** The one line under the title when this step is next, not now. */
		note: string;
		/** The button, which names the place it goes. Empty when the step's
		 *  work happens right here (a card of inputs, the sources) or when
		 *  there is nothing to press yet — a day that has not arrived. */
		verb: string;
		/** Opening the step: in place for the inline ones, elsewhere for the
		 *  letter (a page), the interview (a chat) and the day. */
		go: () => void;
		/** Waving this one away — absent when it cannot be skipped. */
		skip?: { label: string; run: () => void };
		/** The way back once done — the quiet verb at the row's right. */
		back: string;
	}

	const steps = $derived.by(() => {
		const rows: Step[] = [
			{
				id: "letter",
				title: "The founder's letter",
				done: true,
				what: "Why any of this exists, in a page.",
				note: "",
				verb: "Read the letter",
				// A page of its own, not a tab route: the shell's tab router would
				// answer "/founders-letter" with a new chat.
				go: () => void goto("/founders-letter"),
				back: "Read again",
			},
			{
				id: "introductions",
				title: "Introductions",
				done: !!profile?.preferred_name || isDismissed("introductions"),
				what: "What you like to be called, what you'll call it, and when you were born — the few things the record cannot supply.",
				note: "Three questions, in Settings.",
				verb: "",
				go: () => toggle("introductions"),
				skip: { label: "Skip for now", run: () => void dismiss("introductions") },
				back: "Change",
			},
			{
				id: "connect",
				title: "Connect your world",
				done: worldEnough || isDismissed("connect"),
				what: "Your Mac, your phone, your accounts. Your server reads them from here on; this Mac pays off before you stand up.",
				// A collector running with a denied permission outranks the
				// check: a ✓ over a permission hole is how the three-day
				// iMessage outage happened.
				note: degradedNote ?? "Sources, in Settings.",
				verb: "",
				go: () => toggle("connect"),
				skip: { label: "Skip — sources stay in Settings", run: () => void dismiss("connect") },
				back: "Add a source",
			},
		];
		if (accountDone || !store.accountSatisfied) {
			rows.push({
				id: "signin",
				title: "Connect your Virtues account",
				done: accountDone,
				what: "The models it writes with, plus the maps, photos, bank links and calendars. One subscription, metered per request and never kept.",
				note: "One sign-in.",
				verb: "",
				go: () => toggle("signin"),
				back: "Account",
			});
		}
		const interviewDone = store.done("narrative_identity_ready");
		rows.push(
			{
				id: "interview",
				title: "In your own words",
				done: interviewDone || isDismissed("interview"),
				what: "A conversation about your life, about twenty minutes, one question at a time. Stop anywhere; it keeps your place.",
				note: store.interviewStarted && !interviewDone ? "Underway." : "About twenty minutes.",
				verb: store.interviewStarted ? "Continue the interview" : "Start the interview",
				go: () => open(`/chat/${INTERVIEW_CHAT_ID}`, "In your own words"),
				skip: { label: "Skip for now", run: () => void dismiss("interview") },
				back: interviewDone ? "Read it" : "Open",
			},
			{
				id: "first_day",
				title: "Your first day, written up",
				done: firstDay !== null || isDismissed("first_day"),
				what: "Written overnight from what your sources hold. It arrives tomorrow morning, and every day after writes itself.",
				// Pending, with sources flowing: the line says when. Nothing
				// connected yet, no promise — the overnight write needs a day
				// of record to write about.
				note: firstDay !== null ? "Arrived." : worldEnough ? "Written overnight. Tomorrow morning." : "Once a source is flowing.",
				verb: firstDay !== null ? `Read ${prettyDay(firstDay)}` : "",
				go: () => firstDay !== null && open(`/day/day_${firstDay}`, "Your first day"),
				skip: { label: "Don't wait for it here", run: () => void dismiss("first_day") },
				back: firstDay !== null ? `Read ${prettyDay(firstDay)}` : "",
			},
			{
				id: "further",
				title: "Go further",
				done: isDismissed("further"),
				what: "Applets — small programs your server runs for you — and the manual, which says how all of this works.",
				note: "Applets, and the manual.",
				verb: "Open applets",
				go: () => open("/applets", "Applets"),
				skip: { label: "All set", run: () => void dismiss("further") },
				back: "Open",
			},
		);
		return rows;
	});

	/** Open a step in place; opening the open one closes it back to the walk. */
	function toggle(id: StepId) {
		chosen = chosen === id ? null : id;
	}

	/** The first thing still to do — what decides whether this is the page. */
	const next = $derived(steps.find((s) => !s.done) ?? null);
	/** The open step: chosen by hand, else the next one. */
	const now = $derived((chosen ? steps.find((s) => s.id === chosen) : null) ?? next);

	const anything = $derived(next !== null);
	/** Both ends have answered once. */
	const ready = $derived(store.loaded && profileSettled);
	$effect(() => {
		phase = !ready ? "loading" : anything ? "focus" : "settled";
	});

	// ---- the frontispiece: a painting and a line for the step at hand ----
	// The lines are the bank's (agents/build/voice.md), unattributed on the
	// page; the paintings ship in static/plates until the plate job draws
	// them from the record.
	const FRONT: Record<StepId, { src: string; line: string }> = {
		letter: {
			src: "/plates/plate-letter.jpg",
			line: "The trouble with data is not that it is collected, but that it is collected by everyone except its owner.",
		},
		introductions: {
			src: "/plates/plate-introductions.jpg",
			line: "Anything that knows you this well must belong to you.",
		},
		connect: {
			src: "/plates/plate-connect.jpg",
			line: "Most of a life is lost not to anyone's malice, but to nobody writing it down — and the ordinary days, it turns out, were the beautiful ones.",
		},
		signin: {
			src: "/plates/plate-account.jpg",
			line: "The record of a life belongs where the life is lived.",
		},
		interview: {
			src: "/plates/plate-interview.jpg",
			line: "This day, honestly seen, is material enough for virtue. Write to yourself, for yourself — no other reader was ever needed.",
		},
		first_day: {
			src: "/plates/plate-first-day.jpg",
			line: "A life unrecorded scatters; a life recorded, and owned, endures.",
		},
		further: {
			src: "/plates/plate-further.jpg",
			line: "The most revolutionary act available to an ordinary man is an accurate record of his own life, because every power now in existence would prefer he didn't keep one.",
		},
	};
	const front = $derived(FRONT[now?.id ?? "further"]);
	const doneSteps = $derived(steps.filter((s) => s.done));

	onMount(() => {
		void loadProfile();
		void loadCensus();
		// The first day lands on the box's own clock (narration runs at the
		// maintenance hour, once a day) — a five-minute beat is generous.
		const t = setInterval(() => {
			if (document.hidden || firstDay !== null) return;
			void loadCensus();
		}, 300_000);
		// The work happens elsewhere — Sources, You, the interview — so the
		// page re-reads the box each time the person comes back to it.
		const refresh = () => {
			if (document.hidden) return;
			void loadProfile();
			void loadCensus();
			void store.check();
		};
		document.addEventListener("visibilitychange", refresh);
		window.addEventListener("focus", refresh);
		return () => {
			clearInterval(t);
			document.removeEventListener("visibilitychange", refresh);
			window.removeEventListener("focus", refresh);
		};
	});

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

{#if ready && now}
	<div class="spread" in:fade={{ duration: 200 }}>
		<!-- The work: done, now, then, and what it reads from. -->
		<section class="work">
			<h1 class="title">Getting started</h1>
			<p class="sub">Your server is reading your life. Tomorrow morning it writes the first page.</p>

			<!-- The stepper: every step, in walking order, with its state in the
			     rail — a check, the claret numeral for now, a hollow numeral for
			     what follows. The current step opens in place; done steps keep
			     their way back at the right. -->
			<ol class="stepper">
				{#each steps as s, i (s.id)}
					<li class:done={s.done} class:now={s.id === now.id} class:later={!s.done && s.id !== now.id}>
						<div class="rail">
							<span class="dot">{#if s.done}<Icon icon="ri:check-line" width="13" />{:else}{i + 1}{/if}</span>
							{#if i < steps.length - 1}<span class="line"></span>{/if}
						</div>
						<div class="step">
							<div class="row">
								<button class="t" type="button" onclick={s.go}>{s.title}</button>
								{#if s.done && s.back && s.id !== now.id}<button class="back" type="button" onclick={s.go}>{s.back}</button>{/if}
							</div>
							{#if s.id === now.id}
								<div class="body">
									{#if s.id === "introductions"}
										<p class="p">{s.what}</p>
										<IntroductionsCard
											ondone={() => { void loadProfile(); void dismiss("introductions"); chosen = null; }}
											ondismiss={() => { void dismiss("introductions"); chosen = null; }}
										/>
									{:else if s.id === "connect"}
										<p class="p">{s.what}</p>
										<ConnectWorld onConnected={() => void store.check()} onDeviceReady={() => (deviceReady = true)} next="/" />
										{#if !s.done && s.skip}<button class="link quiet" type="button" onclick={s.skip.run}>{s.skip.label}</button>{/if}
									{:else if s.id === "signin"}
										<p class="p">{s.what}</p>
										<div class="gate"><AccountGate done={accountDone} onLinked={() => void store.check()} /></div>
									{:else if s.id === "further"}
										<p class="p">{s.what}</p>
										<div class="acts">
											<button class="btn" type="button" onclick={s.go}>{s.verb}</button>
											<a class="link" href="https://virtues.com/docs" target="_blank" rel="noreferrer">Read the manual</a>
											{#if !s.done && s.skip}<button class="link quiet" type="button" onclick={s.skip.run}>{s.skip.label}</button>{/if}
										</div>
									{:else}
										<p class="p">{s.what}</p>
										<div class="acts">
											{#if s.verb}<button class="btn" type="button" onclick={s.go}>{s.verb}</button>{/if}
											{#if !s.done && s.skip}<button class="link" type="button" onclick={s.skip.run}>{s.skip.label}</button>{/if}
										</div>
									{/if}
								</div>
							{:else if !s.done && s.note}
								<div class="note">{s.note}</div>
							{/if}
						</div>
					</li>
				{/each}
			</ol>

			<div class="foot">
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
			</div>
		</section>

		<!-- The frontispiece: the painting for this step, its line, and the
		     record's numbers. -->
		<Frontispiece
			src={front.src}
			line={front.line}
			figures={census && census.total > 0 ? census.lines.slice(0, 3).map((l) => ({ v: l.count.toLocaleString(), k: l.label })) : []}
			since={census?.earliest ? `The record, since ${new Date(census.earliest).toLocaleDateString(undefined, { month: "long", year: "numeric" })}` : ""}
		/>

	</div>
{/if}

<style>
	/* The spread fills the pane: the work on the left in the page's measure,
	   the painting on the right as a card set in the page's margin. Below
	   900px the painting becomes a header and the work follows. */
	.spread {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(360px, 44%);
		/* Fill the pane: the window less the chrome row and the pane's inset.
		   Nothing above sets a height, so the spread must state its own. */
		min-height: calc(100dvh - var(--chrome-row-h, 40px) - 2 * var(--pane-inset, 12px) - 2px);
		/* No ground of its own: the pane's. A page that paints its own
		   background reads as a different surface from every other page. */
	}
	@media (max-width: 900px) { .spread { grid-template-columns: 1fr; } }

	/* ── the work ── */
	.work { display: flex; flex-direction: column; padding: 56px 56px 40px 64px; min-width: 0; }
	.work > * { animation: arrive 0.5s ease both; }
	.work > :nth-child(2) { animation-delay: 60ms; }
	.work > :nth-child(3) { animation-delay: 120ms; }
	@media (max-width: 640px) { .work { padding: 32px 24px; } }

	.title { font-family: var(--font-serif); font-weight: 400; font-size: 36px; line-height: 1.1; margin: 0; color: var(--color-foreground); }

	/* ── the stepper ── */
	.sub { font-family: var(--font-sans); font-size: 15px; line-height: 1.5; color: var(--color-foreground-muted); margin: 10px 0 0; max-width: 40em; }

	.stepper { list-style: none; margin: 40px 0 0; padding: 0; }
	.stepper li { display: grid; grid-template-columns: 20px minmax(0, 1fr); column-gap: 16px; }
	.rail { display: flex; flex-direction: column; align-items: center; padding-top: 3px; }
	.dot {
		flex: none; width: 20px; height: 20px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center;
		font-family: var(--font-sans); font-size: 11px; font-weight: 500; font-variant-numeric: lining-nums;
		border: 1px solid var(--color-border-strong, var(--color-border)); color: var(--color-foreground-subtle); background: var(--color-surface);
	}
	li.done .dot { background: var(--color-foreground); border-color: var(--color-foreground); color: var(--color-background); }
	li.now .dot { background: var(--color-secondary); border-color: var(--color-secondary); color: #fff; }
	.line { flex: 1; width: 1px; background: var(--color-border); margin: 4px 0; min-height: 12px; }

	.step { padding: 0 0 22px; min-width: 0; }
	li:last-child .step { padding-bottom: 0; }
	.row { display: flex; align-items: baseline; justify-content: space-between; gap: 16px; }
	.t {
		font-family: var(--font-serif); font-weight: 400; font-size: 18px; line-height: 1.3; color: var(--color-foreground);
		background: none; border: 0; padding: 0; text-align: left; min-width: 0; cursor: pointer;
	}
	li.done .t { color: var(--color-foreground-muted); }
	.t:hover { color: var(--color-foreground); }
	li.now .t { font-size: 22px; color: var(--color-foreground); }
	li.later .t { color: var(--color-foreground-subtle); }
	.body { padding-bottom: 8px; }
	.gate { margin-top: 16px; max-width: 34em; }
	.link.quiet { display: block; margin-top: 14px; font-weight: 400; color: var(--color-foreground-subtle); }
	.back { font-family: var(--font-sans); font-size: 13px; color: var(--color-foreground-subtle); background: none; border: 0; padding: 0; cursor: pointer; white-space: nowrap; }
	.back:hover { color: var(--color-primary); }
	.note { font-family: var(--font-sans); font-size: 13px; color: var(--color-foreground-subtle); margin-top: 4px; }
	.p { font-family: var(--font-sans); font-size: 14px; line-height: 1.55; color: var(--color-foreground-muted); margin: 8px 0 0; max-width: 34em; }
	.acts { display: flex; align-items: center; gap: 20px; margin-top: 16px; flex-wrap: wrap; }
	.btn {
		display: inline-flex; align-items: center; height: 40px; padding: 0 22px; border: 0; border-radius: 999px;
		background: var(--color-secondary); color: #fff; font-family: var(--font-sans); font-size: 14px; font-weight: 500;
		cursor: pointer; transition: background 0.15s ease;
	}
	.btn:hover { background: var(--color-secondary-hover, var(--color-secondary)); }
	.link { font-family: var(--font-sans); font-size: 14px; font-weight: 500; color: var(--color-primary); background: none; border: 0; padding: 0; cursor: pointer; text-decoration: none; }
	.link:hover { text-decoration: underline; text-underline-offset: 3px; }

	.foot { margin-top: auto; padding-top: 24px; display: flex; justify-content: flex-end; }
	.door {
		display: flex; align-items: center;
		font-family: var(--font-mono); font-size: 11px;
		color: var(--color-foreground-subtle); background: none; border: 0;
		padding: 2px; cursor: pointer; opacity: 0.45;
		transition: color 0.15s ease, opacity 0.15s ease;
	}
	.door:hover, .door.expanded { color: var(--color-foreground); opacity: 1; }

	/* from-only keyframes, per the house rule */
	@keyframes arrive { from { opacity: 0; transform: translateY(6px); } }
	@media (prefers-reduced-motion: reduce) { .work > * { animation: none; } }
</style>
