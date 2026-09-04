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
	stats row. The facing page is the work: what is done as chips (still
	pressable — the letter is always one click away), the ONE thing to do now
	as a panel with a real button that names the place, what comes next
	beneath it in plain type, and what is done as a quiet row beneath that.

	Four earlier shapes were struck the same day: an accordion whose rows
	unfolded into forms (nobody could tell done from next), a flat list of
	verbs (sent you away without a word), a card floating over a banner
	(a dashboard in a costume), and a book page with shoulder notes (read as
	a dated form). What survived every review was: a picture, a real button,
	the letter within reach, and few words.

	WHILE ANY STEP IS OPEN, THIS IS THE PAGE. HomeView renders nothing else —
	and drops its own title and padding so the spread bleeds to the pane —
	until every step is done or waved away; only then does Home exist. The
	`phase` binding tells HomeView which page this is ("loading" holds until
	both ends have answered once, so a first-run box never flashes Home).

	SKIPPED MEANS GONE FROM THE LIST'S DEMANDS, NOT FROM THE PRODUCT. A
	skipped step counts as settled; the same asks stay findable where they
	permanently live. Dismissals persist on the profile so the page reads the
	same on every glass. The hidden door at the foot skips everything at once.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { fade } from "svelte/transition";
	import Icon from "$lib/components/Icon.svelte";
	import { getProfile, updateProfile, getCensus, type Profile, type Census } from "$lib/api/client";
	import { setupStateStore } from "$lib/stores/setupState.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";

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
	const worldEnough = $derived(store.worldEnough);
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
	type StepId = "letter" | "introductions" | "connect" | "signin" | "interview" | "first_day" | "further";
	interface Step {
		id: StepId;
		title: string;
		done: boolean;
		/** The panel's one paragraph: what this is, in the person's words. */
		what: string;
		/** The one line under the title when this step is next, not now. */
		note: string;
		/** The button, which names the place it goes. Empty when there is
		 *  nothing to press — a day that has not arrived yet. */
		verb: string;
		go: () => void;
		/** Waving this one away — absent when it cannot be skipped. */
		skip?: { label: string; run: () => void };
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
				go: () => open("/founders-letter", "The founder's letter"),
			},
			{
				id: "introductions",
				title: "Introductions",
				done: !!profile?.preferred_name || isDismissed("introductions"),
				what: "What you like to be called, what you'll call it, and when you were born — the few things the record cannot supply.",
				note: "Three questions, in Settings.",
				verb: "Answer",
				go: () => open("/virtues/you", "You"),
				skip: { label: "Skip for now", run: () => void dismiss("introductions") },
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
				verb: "Open sources",
				go: () => open("/sources", "Sources"),
				skip: { label: "Skip for now", run: () => void dismiss("connect") },
			},
		];
		if (accountDone || !store.accountSatisfied) {
			rows.push({
				id: "signin",
				title: "Connect your Virtues account",
				done: accountDone,
				what: "The models it writes with, plus the maps, photos, bank links and calendars. One subscription, metered per request and never kept.",
				note: "One sign-in.",
				verb: "Sign in",
				go: () => open("/virtues/billing", "Plan"),
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
			},
		);
		return rows;
	});

	const doneSteps = $derived(steps.filter((s) => s.done));
	/** The one thing to do now, and the one after it. */
	const now = $derived(steps.find((s) => !s.done) ?? null);
	const then = $derived(now ? (steps.find((s) => !s.done && s.id !== now.id) ?? null) : null);

	const anything = $derived(now !== null);
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
			<div class="head">
				<h1 class="title">Getting started</h1>
			</div>
			<div class="progress" role="progressbar" aria-label="Getting started" aria-valuemin="0" aria-valuemax={steps.length} aria-valuenow={doneSteps.length}>
				{#each steps as s (s.id)}
					<span class:done={s.done} class:now={s.id === now.id}></span>
				{/each}
			</div>

			<div class="now-panel">
				<div class="eyebrow">Now</div>
				<h2 class="t">{now.title}</h2>
				<p class="p">{now.what}</p>
				<div class="acts">
					{#if now.verb}
						<button class="btn" type="button" onclick={now.go}>{now.verb}</button>
					{/if}
					{#if now.skip}
						<button class="link" type="button" onclick={now.skip.run}>{now.skip.label}</button>
					{/if}
				</div>
			</div>

			{#if then}
				<div class="next">
					<div class="eyebrow">Then</div>
					<div class="t">{then.title}</div>
					{#if then.note}<div class="p">{then.note}</div>{/if}
				</div>
			{/if}

			<!-- What is behind you: one quiet row. The chips stay pressable — the
			     letter is always one click away — but carry no check; the row's
			     label already says done. -->
			<div class="rows">
				{#if doneSteps.length > 0}
					<div class="row">
						<span class="lbl">Done</span>
						<div class="chips">
							{#each doneSteps as s (s.id)}
								<button class="chip" type="button" onclick={s.go}>{s.title}</button>
							{/each}
						</div>
					</div>
				{/if}
			</div>

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
		     record's numbers — white on the painting's dusk. -->
		<aside class="front" aria-hidden="true">
			{#key front.src}
				<img src={front.src} alt="" />
			{/key}
			<div class="text">
				<p class="epigraph">{front.line}</p>
				{#if census && census.total > 0 && census.lines.length > 0}
					<div class="ledger">
						{#each census.lines.slice(0, 3) as line (line.id)}
							<div><div class="v">{line.count.toLocaleString()}</div><div class="k">{line.label}</div></div>
						{/each}
					</div>
					{#if census.earliest}
						<div class="since">
							The record, since {new Date(census.earliest).toLocaleDateString(undefined, { month: "long", year: "numeric" })}
						</div>
					{/if}
				{/if}
			</div>
		</aside>

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
	@media (max-width: 900px) {
		.spread { grid-template-columns: 1fr; }
		.front { order: -1; min-height: 320px; margin: 12px; }
	}

	/* ── the painting, as a card in the margin ── */
	.front {
		position: relative; overflow: hidden;
		margin: 20px 20px 20px 0; border-radius: 12px;
		background: color-mix(in srgb, var(--color-foreground) 40%, var(--color-background));
	}
	.front img {
		position: absolute; inset: 0; width: 100%; height: 100%;
		object-fit: cover; object-position: 50% 30%;
		animation: front-in 0.8s ease both;
	}
	.front::after {
		content: ""; position: absolute; inset: 0; pointer-events: none;
		background: linear-gradient(180deg, rgba(20,26,38,0) 38%, rgba(20,26,38,0.55) 68%, rgba(20,26,38,0.86) 100%);
	}
	.front .text {
		position: absolute; left: 0; right: 0; bottom: 0; z-index: 1;
		padding: 0 48px 48px; color: #fff;
		animation: arrive 0.8s ease both; animation-delay: 200ms;
	}
	.epigraph {
		font-family: var(--font-serif); font-weight: 400;
		font-size: 26px; line-height: 1.3; letter-spacing: -0.005em;
		max-width: 15em; margin: 0;
	}
	.ledger { display: flex; gap: 32px; margin-top: 32px; padding-top: 24px; border-top: 1px solid rgba(255,255,255,0.28); }
	.ledger .v { font-family: var(--font-serif); font-size: 28px; line-height: 1; font-variant-numeric: lining-nums tabular-nums; }
	.ledger .k { font-family: var(--font-sans); font-size: 12px; color: rgba(255,255,255,0.72); margin-top: 8px; }
	.since { margin-top: 20px; font-family: var(--font-sans); font-size: 12px; color: rgba(255,255,255,0.6); }

	/* ── the work ── */
	.work { display: flex; flex-direction: column; padding: 56px 56px 40px 64px; min-width: 0; }
	.work > * { animation: arrive 0.5s ease both; }
	.work > :nth-child(2) { animation-delay: 60ms; }
	.work > :nth-child(3) { animation-delay: 120ms; }
	.work > :nth-child(4) { animation-delay: 180ms; }
	.work > :nth-child(5) { animation-delay: 240ms; }
	.work > :nth-child(6) { animation-delay: 300ms; }
	@media (max-width: 640px) { .work { padding: 32px 24px; } .front .text { padding: 0 24px 28px; } }

	.title { font-family: var(--font-serif); font-weight: 400; font-size: 36px; line-height: 1.1; margin: 0; color: var(--color-foreground); }
	.progress { display: grid; grid-auto-flow: column; grid-auto-columns: 1fr; gap: 6px; margin-top: 20px; }
	.progress span { height: 3px; border-radius: 999px; background: var(--color-border); }
	.progress span.done { background: var(--color-foreground); }
	.progress span.now { background: var(--color-secondary); }

	/* The quiet rows: a label, then ghost chips — a hairline, no fill, no
	   check. Pressable, but nothing here competes with the panel above. */
	.rows { margin-top: 48px; display: grid; gap: 12px; }
	.row { display: grid; grid-template-columns: 96px minmax(0, 1fr); align-items: start; }
	.row .chips { display: flex; flex-wrap: wrap; gap: 8px; }
	.lbl { font-family: var(--font-sans); font-size: 13px; line-height: 28px; color: var(--color-foreground-subtle); }
	.chip {
		display: inline-flex; align-items: center; height: 28px; padding: 0 12px;
		border: 1px solid var(--color-border); border-radius: 999px; background: transparent;
		font-family: var(--font-sans); font-size: 13px; font-weight: 400; color: var(--color-foreground-muted);
		cursor: pointer; transition: background 0.15s ease, color 0.15s ease;
	}
	.chip:hover { background: var(--hover-bg, color-mix(in srgb, var(--color-foreground) 7%, transparent)); color: var(--color-foreground); }

	.now-panel {
		margin-top: 40px; padding: 28px 32px;
		background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 12px;
	}
	.eyebrow { font-family: var(--font-sans); font-size: 12px; font-weight: 500; color: var(--color-foreground-subtle); }
	.now-panel .eyebrow { color: var(--color-secondary); }
	.now-panel .t { font-family: var(--font-serif); font-weight: 400; font-size: 30px; line-height: 1.15; margin: 8px 0 0; color: var(--color-foreground); }
	.now-panel .p { font-family: var(--font-sans); font-size: 15px; line-height: 1.55; color: var(--color-foreground-muted); margin: 12px 0 0; max-width: 34em; }
	.acts { display: flex; align-items: center; gap: 20px; margin-top: 24px; flex-wrap: wrap; }
	.btn {
		display: inline-flex; align-items: center; height: 40px; padding: 0 22px; border: 0; border-radius: 999px;
		background: var(--color-secondary); color: #fff; font-family: var(--font-sans); font-size: 14px; font-weight: 500;
		cursor: pointer; transition: background 0.15s ease;
	}
	.btn:hover { background: var(--color-secondary-hover, var(--color-secondary)); }
	.link { font-family: var(--font-sans); font-size: 14px; font-weight: 500; color: var(--color-primary); background: none; border: 0; padding: 0; cursor: pointer; }
	.link:hover { text-decoration: underline; text-underline-offset: 3px; }

	.next { margin-top: 32px; padding: 0 32px; }
	.next .t { font-family: var(--font-serif); font-weight: 400; font-size: 22px; line-height: 1.2; margin-top: 6px; color: var(--color-foreground); }
	.next .p { font-family: var(--font-sans); font-size: 13px; color: var(--color-foreground-subtle); margin-top: 4px; }

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
	@keyframes front-in { from { opacity: 0; } }
	@media (prefers-reduced-motion: reduce) {
		.work > *, .front .text, .front img { animation: none; }
	}
</style>
