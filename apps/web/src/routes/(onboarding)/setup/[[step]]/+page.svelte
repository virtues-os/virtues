<!--
  /setup — Setup, the one flow (agents/plan/setup-plan.md).

  THE STAGE (2026-09-25). Paper and grain are painted once, here
  (`.setup-stage`, setup.css), and every step changes on top of them: steps
  crossfade over each other rather than fading through a blank page. The
  progress is the mark itself (SetupMark), which Welcome's big ∴ flies up
  to become. The four laws are in agents/plan/setup-plan.md.

  Ten steps: Welcome (the cold open, with light or dark), the founder's
  letter, Account, Server, Wi-Fi, Subscription, Names, Connections, Timeline,
  Interview. Everything through Names is required.

  THE FIRST HALF (Account, Server, Wi-Fi, and pairing at Wi-Fi's end) runs
  here only before this device has a server: on the iPhone, whose shell
  opens /setup from its own copy of the app when unpaired, or in dev with
  `?radio=fake` (prepair.svelte.ts). Anywhere else those three are done by
  construction. Sign-in comes first because the account's grant has to
  cross the Bluetooth link before pairing. The last three
  each carry a Skip, and "Finish later" opens the app with whatever is left
  waiting on the rail's Setup tile. Setup ends once, with the dots drawing
  together into the ∴ and the app opening beneath it.

  `/setup` alone decides where to begin: the first step still to do, which
  is Welcome on a new server.
  `/setup/{step}` is a step, so the rail's panel can send someone straight
  back into any of them, and a reload lands where it left off.

  WAS /founders-letter (2026-08-31 to 2026-09-24), which was the letter and,
  at the end, the subscription; the steps after it lived in the app's pane.
  The letter stays readable on its own at /founders-letter.
-->
<script lang="ts">
	import { afterNavigate, goto, replaceState } from "$app/navigation";
	import { page } from "$app/state";
	import { onMount } from "svelte";
	import { fade } from "svelte/transition";
	import "$lib/components/setup/setup.css";
	import { M, rise } from "$lib/components/setup/motion";
	import { isMacOS } from "$lib/utils/platform";
	import { applyTheme, DEFAULT_THEMES, getTheme, isValidTheme, setTheme } from "$lib/utils/theme";
	import { THEME_CHOSEN_KEY } from "$lib/components/setup/themeReveal";
	import Hello from "$lib/components/onboarding/Hello.svelte";
	import FoundersLetter from "$lib/components/onboarding/document/FoundersLetter.svelte";
	import SetupMark from "$lib/components/setup/SetupMark.svelte";
	import ThemeSwitch from "$lib/components/setup/ThemeSwitch.svelte";
	import { setup, isSetupStep, intoLabel, SERVER, type SetupStepId } from "$lib/components/setup/setup.svelte";
	import { score } from "$lib/components/setup/score.svelte";
	import StepPaired from "$lib/components/setup/steps/StepPaired.svelte";
	import StepAccount from "$lib/components/setup/steps/StepAccount.svelte";
	import StepServer from "$lib/components/setup/steps/StepServer.svelte";
	import StepWifi from "$lib/components/setup/steps/StepWifi.svelte";
	import StepPairing from "$lib/components/setup/steps/StepPairing.svelte";
	import { prePair } from "$lib/components/setup/prepair.svelte";
	import { holdStage } from "$lib/components/setup/inApp";
	import StepSubscription from "$lib/components/setup/steps/StepSubscription.svelte";
	import StepNames from "$lib/components/setup/steps/StepNames.svelte";
	import StepConnections from "$lib/components/setup/steps/StepConnections.svelte";
	import StepTimeline from "$lib/components/setup/steps/StepTimeline.svelte";
	import StepInterview from "$lib/components/setup/steps/StepInterview.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { openApp } from "$lib/stores/appOpening";
	import { getProfile, updateProfile, getSetupState, skipOnboarding } from "$lib/api/client";

	let ready = $state(false);
	/** Welcome's dots wait for the cold open to finish drawing the mark. */
	let helloSettled = $state(false);
	// ── the letter ──────────────────────────────────────────────────────
	const letterLabel = $derived(intoLabel(setup.upNext("letter")));
	let settingAside = $state(false);
	/** The pinned way on shows while the letter's own button is out of view. */
	let pinShown = $state(false);
	$effect(() => {
		if (step !== "letter") {
			pinShown = false;
			settingAside = false;
			return;
		}
		let io: IntersectionObserver | null = null;
		const t = setTimeout(() => {
			const exit = document.querySelector(".paper .exit");
			if (!exit || !("IntersectionObserver" in window)) return;
			io = new IntersectionObserver((es) => (pinShown = !es.some((e) => e.isIntersecting)));
			io.observe(exit);
		}, still ? 0 : 2400);
		return () => {
			clearTimeout(t);
			io?.disconnect();
		};
	});
	async function leaveLetter() {
		if (settingAside) return;
		settingAside = true;
		await new Promise((r) => setTimeout(r, still ? 0 : M.base));
		setup.passIntro(2);
		await advance("letter");
	}

	/** Welcome's big ∴, as the ink box the small mark flies up from. */
	let markFrom = $state<DOMRect | null>(null);
	function leaveWelcome() {
		const svg = document.querySelector<SVGSVGElement>(".hello svg.mark");
		if (svg) {
			// Drawn on a -8..32 box around the mark's 24 units.
			const r = svg.getBoundingClientRect();
			const u = r.width / 40;
			markFrom = new DOMRect(r.left + 8 * u, r.top + 8 * u, 24 * u, 24 * u);
		}
		setup.passIntro(1);
		go("letter");
	}

	/** Hello's ∴, drawn on a -8..32 box: where a theme change is pushed out of. */
	function welcomeMark() {
		const el = document.querySelector(".hello svg.mark");
		return el ? { el, origin: -8, span: 40 } : null;
	}


	// ARRIVING ON A STEP PUTS FOCUS ON IT. The button that brought them here
	// is gone, so focus fell to the page, and Tab started from the top of the
	// document. Once the new step has faded in, its heading takes focus (a
	// step that focuses its own field has done so already, and keeps it).
	$effect(() => {
		const s = step;
		if (!ready || !s || s === "welcome") return;
		const t = setTimeout(() => {
			const a = document.activeElement;
			if (a && a !== document.body) return;
			const h = document.querySelector<HTMLElement>("main.flow h1, main.flow h2");
			if (!h) return;
			if (!h.hasAttribute("tabindex")) h.setAttribute("tabindex", "-1");
			h.focus({ preventScroll: true });
		}, still ? 60 : M.base + M.quick + 60);
		return () => clearTimeout(t);
	});

	/** One true line under the name at the close: the letter promised a page
	 *  every morning, and this says when the first one comes (the same rule
	 *  as the server's graduated line). Without devices, what's ready now. */
	const closeLine = $derived.by(() => {
		if (gettingStarted.state?.first_day) return "Your first page is on Home, and there will be one every morning.";
		if (setup.status("connections") === "done") return "Tomorrow morning there will be a page on Home for today.";
		if (setup.status("interview") === "done") return "Your story is ready to read.";
		return null;
	});

	// A failed close is said until they move on.
	$effect(() => {
		void step;
		closeError = null;
	});

	// Welcome replays its opening whenever someone comes back to it.
	$effect(() => {
		if (step !== "welcome") helloSettled = false;
	});

	// The track plays under Welcome and the letter as one scene, and goes
	// quiet once either is left for a step (the letter's button, or a dot).
	$effect(() => {
		if (step && step !== "welcome" && step !== "letter") score.fade();
	});
	$effect(() => () => score.fade(0));
	let closing = $state(false);
	/** The close couldn't let the app open, said where the close was. */
	let closeError = $state<string | null>(null);
	let unreachable = $state(false);

	const still =
		typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

	const param = $derived(page.params.step ?? null);
	const step = $derived<SetupStepId | null>(isSetupStep(param) ? param : null);
	const current = $derived(setup.steps.find((s) => s.id === step) ?? null);

	// FRAMELESS ON THE MAC. Setup runs full-bleed: the title bar becomes an
	// overlay, the traffic lights float over the stage, and a thin strip at
	// the top keeps the window draggable. The app's own windows keep their
	// title bar, so it goes back the moment Setup is left. Silently nothing
	// in a browser, or in an app build without the permission.
	async function titleBar(style: "overlay" | "visible") {
		if (!isMacOS) return;
		try {
			const { getCurrentWindow } = await import("@tauri-apps/api/window");
			await getCurrentWindow().setTitleBarStyle(style);
		} catch {
			/* an older app: the ordinary window is fine */
		}
	}
	onMount(() => {
		void titleBar("overlay");
		return () => void titleBar("visible");
	});

	/** What this copy knows that the server's can't read: how far through the
	 *  reading they got, and a theme, only if one was picked on Welcome. A
	 *  followed system mode or this copy's fallback stays here, so the server
	 *  keeps its own rather than saving a choice nobody made. */
	function carried(): Record<string, string> {
		const out: Record<string, string> = { intro: String(setup.introStage) };
		try {
			if (localStorage.getItem(THEME_CHOSEN_KEY)) out.theme = getTheme();
		} catch {
			/* no theme to carry */
		}
		return out;
	}

	/** What the Mac's own copy handed over (`prePair.handOff`): taken once,
	 *  then dropped from the address. */
	function takeCarried() {
		const q = page.url.searchParams;
		const intro = Number(q.get("intro"));
		if (intro === 1 || intro === 2) setup.passIntro(intro);
		const theme = q.get("theme");
		if (theme && isValidTheme(theme)) {
			try {
				localStorage.setItem(THEME_CHOSEN_KEY, "1");
			} catch {
				/* saved on the server below all the same */
			}
			// Applied again once saved: the layout's own read of the server's
			// theme can land between the two and paint the old one.
			void setTheme(theme).then(() => applyTheme(theme));
		}
		// After the router has started (SvelteKit refuses before, in dev).
		if (q.has("intro") || q.has("theme")) setTimeout(() => replaceState(page.url.pathname, page.state), 0);
	}

	/** A device with no server has no theme of its own yet: it follows the
	 *  system's dark mode until someone picks on Welcome. The pick (or this)
	 *  rides the hand-off and is saved once there is a server to save it.
	 *  With a server, its stored theme stands. */
	function followSystemTheme() {
		if (!prePair.active) return;
		try {
			if (localStorage.getItem(THEME_CHOSEN_KEY)) return;
			const dark = window.matchMedia?.("(prefers-color-scheme: dark)").matches;
			applyTheme(DEFAULT_THEMES[dark ? "dark" : "light"]);
		} catch {
			/* the light default stands */
		}
	}

	// The stage is up: its own steps stay on it to the close, even for someone
	// whose `/setup` would otherwise open in the app (a replay from Welcome).
	onMount(() => {
		holdStage(true);
		return () => holdStage(false);
	});

	onMount(() => {
		followSystemTheme();
		takeCarried();
		void (async () => {
			// No server yet: the first half runs here, and there is nothing
			// to reach until it pairs.
			if (prePair.active) {
				ready = true;
				route();
				return;
			}
			await setup.refresh();
			if (!gettingStarted.loaded || (!gettingStarted.state && !gettingStarted.unsupported)) {
				unreachable = true;
				return;
			}
			void captureTimezone();
			ready = true;
			route();
		})();
	});

	// A step in the URL that cannot be entered yet (typed, or a stale link)
	// goes to where setup actually stands; an unknown one to the start.
	$effect(() => {
		if (!ready || closing) return;
		if (param && !step) {
			void goto("/setup", { replaceState: true });
		} else if (current && !current.reachable) {
			void goto(`/setup/${setup.resumeAt ?? "subscription"}`, { replaceState: true });
		}
	});

	function route() {
		if (step) return;
		const at = setup.resumeAt;
		if (at) void goto(`/setup/${at}`, { replaceState: true });
		else void finish();
	}

	// The server's home zone, from this browser, where the server has none or
	// reads UTC (a datacenter box). A real appliance keeps its own. The Names
	// step refines it with a city. See agents/record/timezone-model.md.
	async function captureTimezone() {
		try {
			const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
			if (!tz) return;
			const p = await getProfile();
			if (!p.home_timezone || p.home_timezone === "UTC") {
				if (p.home_timezone !== tz) await updateProfile({ home_timezone: tz });
			}
		} catch {
			/* non-essential */
		}
	}

	function go(id: SetupStepId) {
		void goto(`/setup/${id}`);
	}

	/** On to the next step still to do. Steps already done are passed over:
	 *  in the web app Server and Wi-Fi always are, and a subscription linked
	 *  before pairing is too, so their receipts never interrupt the walk.
	 *  Any done step can still be opened from its dot. */
	async function advance(from: SetupStepId) {
		await setup.refresh();
		const steps = setup.steps;
		const i = steps.findIndex((s) => s.id === from);
		const next = steps.slice(i + 1).find((s) => s.status !== "done");
		if (next) go(next.id);
		else await finish();
	}

	/** The link is gone or they chose another server: back to finding it. */
	async function lost() {
		await prePair.forget();
		go("server");
	}

	/**
	 * Paired: the second half reads the server, in place, so the mark and the
	 * stage carry on without a reload. If the server can't be read yet (the
	 * phone's connection to it is seconds old), reload into Setup, which
	 * then starts from wherever the server says things stand.
	 */
	async function paired() {
		// A computer carries on in the server's own copy of the app, told how
		// far through the reading this one got and the theme picked on Welcome.
		const away = await prePair.handOff(carried());
		if (away === "away") return;
		if (away === "silent") throw new Error("silent");
		await setup.refresh();
		if (!gettingStarted.loaded || (!gettingStarted.state && !gettingStarted.unsupported)) {
			location.replace("/setup");
			return;
		}
		const at = setup.resumeAt;
		if (at) go(at);
		else await finish();
		// The pairing screen stays up until the next step has faded in over it.
		setTimeout(() => (prePair.handingOver = false), still ? 0 : M.base + M.quick);
	}

	async function skip(id: SetupStepId) {
		const key = SERVER[id];
		try {
			if (key) await gettingStarted.skip(key, true);
		} catch {
			/* moving on never waits on the server */
		}
		await advance(id);
	}

	/**
	 * THE CLOSE. The dots draw together into the ∴ in the middle of the
	 * screen, then the app opens beneath it. The app shell holds anyone out
	 * while `onboarding_status` is not active, so that is released first —
	 * leaving without saying "on purpose" would bounce straight back here.
	 */
	async function finish() {
		if (closing) return;
		closing = true;
		closeError = null;
		try {
			const s = await getSetupState();
			// `active` is what the gate lets through, whatever else is true.
			if (s.onboarding_status !== "active") await skipOnboarding(true);
		} catch {
			// Opening the app anyway only bounced back here, onto this same
			// page still mid-close, with nothing to press. So say it and stay.
			closing = false;
			closeError = "Your server couldn't finish Setup. Check your connection, then try again.";
			return;
		}
		await new Promise((r) => setTimeout(r, still ? 0 : 2300));
		// The app opens on page one of what Setup made: the story told in the
		// interview ("You"), else the chapters drawn, else home.
		const dest =
			setup.status("interview") === "done"
				? "/wiki/identity"
				: setup.status("timeline") === "done"
					? "/wiki/chapters"
					: "/home";
		await titleBar("visible");
		await openApp(dest);
	}

	// The app's gate can still send the close back here (the server changed
	// its mind between the two reads). This page is reused, not remounted, so
	// it has to leave the close itself.
	afterNavigate((nav) => {
		if (!closing || nav.type === "popstate" || !nav.to?.url.pathname.startsWith("/setup")) return;
		closing = false;
		closeError = "Your server still has Setup open. Try again.";
		void titleBar("overlay");
		// Bounced to bare /setup: stand on the step still open, not an
		// empty stage. (Not `route()`, which would close again and loop.)
		if (!step && setup.resumeAt) void goto(`/setup/${setup.resumeAt}`, { replaceState: true });
	});
</script>

<svelte:head>
	<title>Setup</title>
</svelte:head>

<!-- The paper is down before anything loads, so the first frame is the
     stage, never a blank window. -->
<div class="setup-stage" aria-hidden="true"></div>
{#if isMacOS}<div class="drag" data-tauri-drag-region aria-hidden="true"></div>{/if}

{#if unreachable}
	<div class="center">
		<!-- Over the relay "the same network" is not the fix, and was untrue. -->
		<p class="err" in:fade>Couldn't reach your server. Check that it's on and connected to the internet, then try again.</p>
		<button type="button" class="setup-go" in:fade onclick={() => location.reload()}>Try again</button>
	</div>
{:else if !ready}
	<div class="center" aria-hidden="true"></div>
{:else}
	<SetupMark
		steps={setup.steps}
		current={closing ? null : step}
		{closing}
		hidden={step === "welcome"}
		from={markFrom}
		onpick={go}
	/>

	<!-- THE CLOSE HAS A NAME (2026-09-28). The ∴ came down to the middle and
	     held still for two seconds, saying nothing; now the assistant's name
	     rises under it as it settles, so what the person meets in the app has
	     been introduced, and the app opens beneath the two together. -->
	{#if closing}
		<p class="close-name" in:rise={{ delay: still ? 0 : 900, duration: still ? 0 : M.slow }}>{setup.assistantName}</p>
		{#if closeLine}
			<p class="close-line" in:rise={{ delay: still ? 0 : 1250, duration: still ? 0 : M.slow }}>{closeLine}</p>
		{/if}
	{/if}

	<!-- Not on a finished step: there, "Finish later" means the same as the
	     step's own way on, and the last step's is "Finish setup". -->
	{#if closeError}
		<div class="close-error" role="alert" transition:fade={{ duration: 200 }}>
			<p>{closeError}</p>
			<button type="button" class="setup-past" onclick={finish}>Try again</button>
		</div>
	{/if}

	{#if current?.optional && current.status !== "done" && !closing}
		<button type="button" class="later" onclick={finish} transition:fade={{ duration: 200 }}>Finish later</button>
	{/if}

	<!-- The corner: sound while the track plays, and on Welcome the one
	     look preference, kept small and out of the lockup's way. -->
	<div class="corner">
	{#if step === "welcome" && helloSettled}
		<span transition:fade={{ duration: 300 }}><ThemeSwitch mark={welcomeMark} /></span>
	{/if}
	{#if score.playing && (step === "letter" || (step === "welcome" && helloSettled))}
		<button
			type="button"
			class="sound"
			onclick={() => score.toggleMute()}
			aria-label={score.muted ? "Turn sound on" : "Turn sound off"}
			transition:fade={{ duration: 300 }}
		>
			<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
				<path d="M4 9.5h3.5L12 6v12l-4.5-3.5H4z" />
				{#if score.muted}
					<path d="M16 9.5l5 5M21 9.5l-5 5" />
				{:else}
					<path d="M15.5 9.2a4 4 0 0 1 0 5.6M18 7a7 7 0 0 1 0 10" />
				{/if}
			</svg>
		</button>
	{/if}
	</div>

	<main class="flow" class:closing>
		{#if step}
			{#key step}
				<!-- Opacity only: a transform here would carry Welcome's fixed
				     layer with it. The two leaves share one grid cell, so the
				     outgoing one fades over the incoming. -->
				<div
					class="leaf"
					in:fade={{ duration: still ? 0 : M.base, delay: still ? 0 : M.quick }}
					out:fade={{ duration: still ? 0 : M.quick }}
				>
					{#if step === "welcome"}
						<Hello onsettle={() => (helloSettled = true)} onnext={leaveWelcome} continueLabel="Begin" />
					{:else if step === "letter"}
						<!-- THE LETTER IS A SHEET OF PAPER set on the stage: it
						     arrives, is read, and is set aside. The way on is pinned
						     at the foot until the letter's own button comes into
						     view, so it is never two screens away. -->
						<div class="letter-stage">
							<article class="paper" class:aside={settingAside}>
								<FoundersLetter beginLabel={letterLabel} onbegin={leaveLetter} />
							</article>
						</div>
						{#if pinShown && !settingAside}
							<div class="pin-fade" aria-hidden="true" transition:fade={{ duration: M.base }}></div>
							<button type="button" class="setup-go pin" onclick={leaveLetter} transition:fade={{ duration: M.base }}>
								{letterLabel}
								<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M5 12h13M13 6.5 18.5 12 13 17.5" /></svg>
							</button>
						{/if}
					{:else if step === "account" && prePair.active}
						<StepAccount onnext={() => advance("account")} />
					{:else if step === "server" && prePair.active}
						<StepServer onnext={() => advance("server")} />
					{:else if step === "wifi" && (prePair.active || prePair.handingOver)}
						<!-- Without an open link (a reload drops it) Wi-Fi isn't
						     reachable, and the step guard sends it back to Server. -->
						{#if (prePair.link && prePair.online) || prePair.handingOver}
							<StepPairing onpaired={paired} onlost={lost} />
						{:else if prePair.link}
							<StepWifi link={prePair.link} onjoined={() => (prePair.online = true)} onlost={lost} />
						{/if}
					{:else if step === "account" || step === "server" || step === "wifi"}
						<StepPaired which={step} onnext={() => advance(step)} />
					{:else if step === "subscription"}
						<StepSubscription onnext={() => advance("subscription")} />
					{:else if step === "names"}
						<StepNames onnext={() => advance("names")} />
					{:else if step === "connections"}
						<StepConnections onnext={() => advance("connections")} />
					{:else if step === "timeline"}
						<StepTimeline onnext={() => advance("timeline")} onskip={() => skip("timeline")} />
					{:else if step === "interview"}
						<StepInterview
							onnext={() => advance("interview")}
							onskip={() => skip("interview")}
							ondraw={() => go("timeline")}
						/>
					{/if}
				</div>
			{/key}
		{/if}
	</main>
{/if}

<style>
	.drag {
		position: fixed;
		z-index: 5;
		inset: 0 0 auto 0;
		height: 28px;
	}
	.center {
		position: relative;
		z-index: 1;
		min-height: 100vh;
		display: grid;
		place-items: center;
		align-content: center;
		gap: 16px;
		padding: 0 16px;
	}
	.err {
		max-width: 28rem;
		margin: 0;
		font-size: 15px;
		line-height: 1.5;
		text-align: center;
		text-wrap: balance;
		color: var(--color-foreground);
	}

	/* The steps sit under the dots, which are fixed at the top. */
	.flow {
		position: relative;
		z-index: 1;
		min-height: 100vh;
		padding-top: 64px;
		display: grid;
		transition:
			opacity 500ms ease,
			filter 500ms ease;
	}
	.flow.closing {
		opacity: 0;
		filter: blur(4px);
		pointer-events: none;
	}
	.leaf {
		grid-area: 1 / 1;
		display: flex;
		flex-direction: column;
	}

	.close-line {
		position: fixed;
		z-index: 60;
		left: 16px;
		right: 16px;
		top: calc(50% + 128px);
		margin: 0;
		text-align: center;
		font-size: 15px;
		color: var(--color-foreground-muted);
		text-wrap: balance;
		pointer-events: none;
	}
	.close-name {
		position: fixed;
		z-index: 60;
		left: 0;
		right: 0;
		/* Under the ∴, which closes centered at 4.5× its 26px. */
		top: calc(50% + 76px);
		margin: 0;
		text-align: center;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 32px;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		pointer-events: none;
	}
	/* A heading holds focus only to place the reader; it isn't a control. */
	.flow :global(h1[tabindex="-1"]:focus),
	.flow :global(h2[tabindex="-1"]:focus) {
		outline: none;
	}
	.close-error {
		position: fixed;
		z-index: 21;
		left: 16px;
		right: 16px;
		bottom: max(24px, env(safe-area-inset-bottom));
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: center;
		gap: 4px 12px;
		text-align: center;
		font-size: 14px;
		color: var(--color-error);
	}
	.close-error p {
		margin: 0;
	}
	.later {
		position: fixed;
		z-index: 21;
		top: max(22px, env(safe-area-inset-top));
		right: 16px;
		padding: 0.35rem 0.1rem;
		border: none;
		background: none;
		font: inherit;
		font-size: 13px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: color 0.15s ease;
	}
	.later:hover {
		color: var(--color-foreground);
	}
	.later:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 3px;
		border-radius: 0;
	}

	.corner {
		position: fixed;
		z-index: 61;
		/* The buttons grew to 44 around the same glyphs; the corner moved
		   out by the difference so the glyphs stay where they were. */
		top: max(11px, env(safe-area-inset-top));
		right: 11px;
		display: flex;
		gap: 0;
	}
	/* On a phone the nine dots take most of the top line; the corner
	   tightens so it clears them down to a 360px screen. */
	@media (max-width: 440px) {
		.corner {
			top: max(16px, env(safe-area-inset-top));
			right: 8px;
			gap: 0;
		}
		.corner :global(.flip),
		.corner .sound {
			position: relative;
			width: 30px;
			height: 30px;
		}
		/* Narrow to leave the dots their line, but a thumb's height. */
		.corner :global(.flip)::after,
		.corner .sound::after {
			content: "";
			position: absolute;
			inset: -7px 0;
		}
	}
	.sound {
		display: grid;
		place-content: center;
		width: 44px;
		height: 44px;
		border: none;
		border-radius: 50%;
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}
	/* "Finish later" is small to look at, but a thumb gets 44 points. */
	.later::after {
		content: "";
		position: absolute;
		inset: -8px;
	}
	.sound:hover {
		color: var(--color-foreground);
	}
	.sound:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}
	.sound svg {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.6;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	/* ── the letter ──────────────────────────────────────────────────── */
	.letter-stage {
		display: flex;
		justify-content: center;
		padding: 1.5rem 16px 7rem;
	}
	/* A sheet: a little lighter than the stage, a hairline edge and a soft
	   shadow under it, so it reads as paper set down on paper. On a wide
	   window it widens to hold the letter's margin notes on the sheet. */
	.paper {
		width: 100%;
		max-width: 46rem;
		padding: clamp(2rem, 6vw, 4.5rem) clamp(1.25rem, 6vw, 4.5rem) clamp(2.5rem, 6vw, 4.5rem);
		border-radius: 6px;
		/* The overlay color is the one every theme designs to sit above its
		   page; the shadow and hairline do the lifting. */
		background: var(--color-surface-overlay, var(--color-surface));
		outline: 1px solid color-mix(in srgb, var(--color-foreground) 9%, transparent);
		outline-offset: -1px;
		animation: sheet-in 900ms var(--m-spring) both;
		transition:
			transform var(--m-base) var(--m-ease),
			opacity var(--m-base) var(--m-ease);
	}
	@media (min-width: 76rem) {
		.paper {
			max-width: calc(38rem + 2.5rem + 15rem + 9rem);
			padding-right: calc(4.5rem + 17.5rem);
		}
	}
	@keyframes sheet-in {
		from {
			opacity: 0;
			transform: translateY(40px) rotate(-0.4deg);
		}
	}
	/* Set aside: up a little, smaller, gone. */
	.paper.aside {
		opacity: 0;
		transform: translateY(-18px) scale(0.985);
	}
	/* The letter dissolves under the pinned button rather than running
	   behind it. */
	.pin-fade {
		position: fixed;
		z-index: 19;
		left: 0;
		right: 0;
		bottom: 0;
		height: 140px;
		pointer-events: none;
		background: linear-gradient(to bottom, transparent, var(--color-surface) 70%);
	}
	.pin {
		position: fixed;
		z-index: 20;
		left: 50%;
		bottom: max(24px, env(safe-area-inset-bottom));
		transform: translateX(-50%);
		/* A fixed box at left: 50% only has half the window to wrap in. */
		white-space: nowrap;
	}
	.pin:active:not(:disabled) {
		transform: translateX(-50%) scale(0.97);
	}
	.pin svg {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.6;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	@media (prefers-reduced-motion: reduce) {
		.paper {
			animation: none;
			transition: none;
		}
		.flow {
			transition: none;
		}
	}
</style>
