<!--
  /setup — Setup, the one flow (agents/build/onboarding.md, "Setup").

  THE STAGE (2026-09-25). Paper and grain are painted once, here
  (`.setup-stage`, setup.css), and every step changes on top of them: steps
  crossfade over each other rather than fading through a blank page. The
  progress is the mark itself (SetupMark), which Welcome's big ∴ flies up
  to become. The four rules are in agents/build/onboarding.md, "Four rules for the setup flow".

  The steps, in `ORDER` (setup.svelte.ts): Welcome (the cold open, with light
  or dark), Server, Names, Connections, Subscription. Connections can be
  skipped; the rest are required. The founder's letter is not a step: the
  pair code lasts about 15 minutes, so it waits at the close as "A note
  from us", and stays readable in the app and at /founders-letter.

  THE DESK. Every step after Welcome sits on a sheet (the stage's paper,
  lifted by a hairline and a shadow), with the Tuscany print faded along the
  bottom of the window, drawn in the theme's ink (`.setup-print`).

  THE FIRST HALF (Server: finding it and pairing) runs here only before this
  device has a server: on the iPhone and the Mac, whose shells open /setup
  from their own copy of the app when unpaired, or in dev with `?radio=fake`
  (prepair.svelte.ts). Anywhere else it is done by design. Pairing needs no
  account; the subscription step signs in. Connections carries a Skip, and "Finish later" opens the app with it
  waiting on the rail's Setup tile. Setup ends once, at Arrival: the dots
  draw together into the ∴, the ink print gives way to the painting in
  color, and the person opens the app themselves.

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
	import { applyTheme, DEFAULT_THEMES, isValidTheme, setTheme } from "$lib/utils/theme";
	import { THEME_CHOSEN_KEY } from "$lib/components/setup/themeReveal";
	import Hello from "$lib/components/onboarding/Hello.svelte";
	import SetupMark from "$lib/components/setup/SetupMark.svelte";
	import ThemeSwitch from "$lib/components/setup/ThemeSwitch.svelte";
	import { setup, isSetupStep, intoLabel, type SetupStepId } from "$lib/components/setup/setup.svelte";
	import { score } from "$lib/components/setup/score.svelte";
	import StepPaired from "$lib/components/setup/steps/StepPaired.svelte";
	import StepServer from "$lib/components/setup/steps/StepServer.svelte";
	import { prePair } from "$lib/components/setup/prepair.svelte";
	import { holdStage } from "$lib/components/setup/inApp";
	import StepSubscription from "$lib/components/setup/steps/StepSubscription.svelte";
	import StepNames from "$lib/components/setup/steps/StepNames.svelte";
	import StepConnections from "$lib/components/setup/steps/StepConnections.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { openApp } from "$lib/stores/appOpening";
	import { getProfile, updateProfile, getSetupState, skipOnboarding } from "$lib/api/client";
	import { getTheme, isThemeDark } from "$lib/utils/theme";
	import { reloadApp } from "$lib/reload";

	let ready = $state(false);
	/** Welcome's dots wait for the cold open to finish drawing the mark. */
	let helloSettled = $state(false);
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
		void advance("welcome");
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

	/** One true line under the name at the close. The letter promised the
	 *  days written up; a day is only written when it has enough in it (a
	 *  recording, or messages: `day_article::has_substance`), so the line
	 *  promises it only once those are arriving. Otherwise, what's ready. */
	const closeLine = $derived.by(() => {
		if (gettingStarted.state?.first_day) return "Your server wrote up your first day, and Home links to it.";
		const arriving = (name: string) => (setup.streams.find((s) => s.name === name)?.count_7d ?? 0) > 0;
		if (arriving("communication_message") || arriving("communication_transcription"))
			return "From tomorrow morning, your server writes up each day it has enough of.";
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

	// The track plays under Welcome and goes quiet once it is left.
	$effect(() => {
		if (step && step !== "welcome") score.fade();
	});
	$effect(() => () => score.fade(0));
	let closing = $state(false);
	/** "Finish later" ends Setup, so it is offered only on an optional step
	 *  with no required step still open after it: on Connections, the
	 *  subscription still follows, and ending there opened the app with no
	 *  AI to think with. That step's own "Skip for now" moves on instead. */
	const laterOk = $derived.by(() => {
		if (!current?.optional || current.status === "done") return false;
		const i = setup.steps.findIndex((s) => s.id === current.id);
		return !setup.steps.slice(i + 1).some((s) => !s.optional && s.status !== "done");
	});
	/** The print runs quieter on a dark ground, where light line work reads
	 *  loud (as Welcome's does). */
	let darkGround = $state(false);
	onMount(() => {
		const read = () => (darkGround = isThemeDark(getTheme()));
		read();
		window.addEventListener("themechange", read);
		return () => window.removeEventListener("themechange", read);
	});
	/** The server step stays up through its paired moment, after pairing
	 *  has made this device paired (StepServer's `onpairing`). */
	let holdServer = $state(false);
	/** The close went through: Arrival's way in is offered. */
	let arrived = $state(false);
	let entering = $state(false);
	const ownerName = $derived(setup.profile?.preferred_name?.trim() || "");
	/** What the assistant has started reading, once anything has arrived:
	 *  shown at the foot of the steps after this device's, so the person sees
	 *  the work begin while they finish. Words only, never a count. */
	const reading = $derived.by(() => {
		// Connections says it on its own sheet; the line carries it after.
		if (step !== "subscription") return null;
		const what = [setup.arriving("computer"), setup.arriving("phone")].filter(Boolean).join(", and ");
		return what ? `${setup.assistantName} is reading ${what}` : null;
	});
	/** The close couldn't let the app open, said where the close was. */
	let closeError = $state<string | null>(null);
	let unreachable = $state(false);
	// `/reconnect` runs on the app's own copy: the phone and the Mac.
	const canDiagnose =
		typeof window !== "undefined" &&
		!!((window as unknown as Record<string, unknown>).__VIRTUES_BACKEND_ORIGIN__ ||
			(window as unknown as Record<string, unknown>).__VIRTUES_MOBILE__);

	const still =
		typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

	const param = $derived(page.params.step ?? null);
	const step = $derived<SetupStepId | null>(isSetupStep(param) ? param : null);
	const current = $derived(setup.steps.find((s) => s.id === step) ?? null);

	/** What a copy of the app at another origin handed over in the address:
	 *  how far through the reading they got, and the theme picked on Welcome.
	 *  Taken once, then dropped from the address.
	 *  TODO(2026-11-15): nothing sends these since the Mac pairs in its own
	 *  copy; delete this and the `?intro`/`?theme` rule in inApp.ts unless the
	 *  Windows app's own copy needs a hand-off (onboarding-diy-plan.md, slice 3). */
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
	 *  in the web app Server always is, and a subscription linked
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

	/**
	 * Paired by address and code: wait for the server to answer here (this
	 * device's connection to it is seconds old), then go on. A new server
	 * goes to Names; a server that is already set up still gives this device
	 * its own step, for its permissions. A server that never answers reloads
	 * into Setup, which says it can't reach it.
	 */
	async function pairedByAddress() {
		const until = Date.now() + 60_000;
		for (;;) {
			await setup.refresh();
			if (gettingStarted.loaded && (gettingStarted.state || gettingStarted.unsupported)) break;
			if (Date.now() > until) {
				location.replace("/setup");
				return;
			}
			await new Promise((r) => setTimeout(r, 1500));
		}
		// The hold ends only once the next step is on screen: released first,
		// the paired receipt (StepPaired) drew for a frame under the move.
		await goto(`/setup/${setup.status("names") === "open" ? "names" : "connections"}`);
		holdServer = false;
	}

	/**
	 * THE CLOSE, which is Arrival. The dots draw together into the ∴, the ink
	 * print gives way to the painting in color, and the greeting rises; the
	 * person opens the app (or the letter) themselves. The app shell holds
	 * anyone out while `onboarding_status` is not active, so that is released
	 * first — leaving without saying "on purpose" would bounce straight back
	 * here.
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
		arrived = true;
	}

	/** Leave Arrival for the app, or for the letter. */
	async function enter(to: "/chat" | "/founders-letter") {
		if (entering) return;
		entering = true;
		try {
			await openApp(to);
		} catch (e) {
			// A failed open leaves both ways in pressable, with the reason.
			console.error("[setup] opening from Arrival failed", e);
			entering = false;
			closeError = "Virtues couldn't open. Try again.";
		}
	}

	// Arrival takes focus once its way in has risen, so a keyboard or screen
	// reader lands on it rather than on the steps that faded out under it.
	let openButton = $state<HTMLButtonElement | null>(null);
	$effect(() => {
		if (!arrived || !openButton) return;
		const t = setTimeout(() => openButton?.focus({ preventScroll: true }), still ? 0 : 1300);
		return () => clearTimeout(t);
	});

	// The app's gate can still send the close back here (the server changed
	// its mind between the two reads). This page is reused, not remounted, so
	// it has to leave the close itself.
	afterNavigate((nav) => {
		if (!closing || nav.type === "popstate" || !nav.to?.url.pathname.startsWith("/setup")) return;
		closing = false;
		arrived = false;
		entering = false;
		closeError = "Your server still has Setup open. Try again.";
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
<!-- The print along the foot of the window, in the theme's ink. Welcome
     draws its own, larger; Arrival trades it for the painting. -->
<div
	class="setup-print"
	class:shown={ready && !unreachable && !!step && step !== "welcome" && !closing}
	class:dark={darkGround}
	aria-hidden="true"
>
	<div class="ink"></div>
</div>
{#if closing}
	<!-- AVIF where the browser takes it (200 KB), WebP on iOS 15 and older
	     macOS (174 KB), the JPEG last (508 KB). -->
	<picture>
		<source srcset="/scenes/lake.avif" type="image/avif" />
		<source srcset="/scenes/lake.webp" type="image/webp" />
		<img class="arrival-scene" src="/scenes/lake.jpg" alt="" aria-hidden="true" decoding="async" />
	</picture>
	<div class="arrival-veil" aria-hidden="true"></div>
{/if}
<!-- The Mac app's title bar is an overlay, so the traffic lights float over
     the stage and this strip keeps the window draggable. -->
{#if isMacOS}<div class="drag" data-tauri-drag-region aria-hidden="true"></div>{/if}

{#if unreachable}
	<div class="center">
		<!-- Over the relay "the same network" is not the fix, and was untrue. -->
		<p class="err" in:fade>Couldn't reach your server. Check that it's on and connected to the internet, then try again.</p>
		<button type="button" class="setup-go" in:fade onclick={() => reloadApp()}>Try again</button>
		{#if canDiagnose}
			<!-- The reconnect screen tells this device being offline from the
			     server being off or refusing it, and can pair again. -->
			<button type="button" class="diagnose" in:fade onclick={() => goto("/reconnect")}>See what's wrong</button>
		{/if}
	</div>
{:else if !ready}
	<div class="center" aria-hidden="true"></div>
{:else}
	<SetupMark
		steps={setup.steps}
		current={closing ? null : step}
		{closing}
		hidden={step === "welcome" && !closing}
		from={markFrom}
		onpick={go}
	/>

	<!-- THE CLOSE HAS A NAME (2026-09-28). The ∴ came down to the middle and
	     held still for two seconds, saying nothing; now the assistant's name
	     rises under it as it settles, so what the person meets in the app has
	     been introduced, and the app opens beneath the two together. -->
	{#if closing && arrived}
		<div class="arrival" role="status">
			<h1 class="arrival-title" in:rise={{ delay: still ? 0 : 700, duration: still ? 0 : M.slow, y: 12 }}>
				Welcome home{ownerName ? `, ${ownerName}` : ""}.
			</h1>
			<p class="arrival-line" in:rise={{ delay: still ? 0 : 900, duration: still ? 0 : M.slow }}>
				{closeLine ?? `${setup.assistantName} is ready when you are.`}
			</p>
			<div class="arrival-ways" in:rise={{ delay: still ? 0 : 1100, duration: still ? 0 : M.slow }}>
				<button type="button" class="setup-go" bind:this={openButton} onclick={() => enter("/chat")} disabled={entering}>Open Virtues</button>
				<button type="button" class="setup-past" onclick={() => enter("/founders-letter")} disabled={entering}>A note from us</button>
			</div>
		</div>
	{/if}

	<!-- A failed close, or a failed open from Arrival, said where it happened.
	     step's own way on, and the last step's is "Finish setup". -->
	{#if closeError}
		<div class="close-error" role="alert" transition:fade={{ duration: 200 }}>
			<p>{closeError}</p>
			<button type="button" class="setup-past" onclick={finish}>Try again</button>
		</div>
	{/if}

	{#if reading && !closing && !closeError}
		<p class="reading" role="status" transition:fade={{ duration: 300 }}>
			<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true">
				<circle cx="12" cy="5" r="2.8" />
				<circle cx="4.5" cy="18" r="2.8" />
				<circle cx="19.5" cy="18" r="2.8" />
			</svg>
			{reading}
		</p>
	{/if}

	{#if laterOk && !closing}
		<button type="button" class="later" onclick={finish} transition:fade={{ duration: 200 }}>Finish later</button>
	{/if}

	<!-- The corner: sound while the track plays, and on Welcome the one
	     look preference, kept small and out of the lockup's way. -->
	<div class="corner">
	{#if step === "welcome" && helloSettled && !closing}
		<span transition:fade={{ duration: 300 }}><ThemeSwitch mark={welcomeMark} /></span>
	{/if}
	{#if score.playing && step === "welcome" && helloSettled && !closing}
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

	<main class="flow" class:closing inert={closing}>
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
					{:else if step === "server" && (prePair.active || holdServer)}
						<StepServer onpairing={() => (holdServer = true)} onpaired={pairedByAddress} />
					{:else if step === "server"}
						<StepPaired onnext={() => advance("server")} />
					{:else if step === "subscription"}
						<StepSubscription onnext={() => advance("subscription")} />
					{:else if step === "names"}
						<StepNames onnext={() => advance("names")} />
					{:else if step === "connections"}
						<StepConnections onnext={() => advance("connections")} />
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
	.diagnose {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: color 0.15s ease;
	}
	.diagnose:hover,
	.diagnose:focus-visible {
		color: var(--color-foreground);
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
		/* One column the window's width: a step's widest line wraps instead
		   of widening the column past a phone's screen. */
		grid-template-columns: minmax(0, 1fr);
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
	/* On a phone the ∴ and its step name take the top line's middle; the corner
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

	/* ── the desk ─────────────────────────────────────────────────────── */
	/* The print along the foot of the window, drawn in the theme's ink (a
	   mask over the primary color, as Welcome draws it), faded toward the
	   top so a sheet's lower edge sits in it rather than on it. */
	.setup-print {
		position: fixed;
		z-index: 0;
		left: 0;
		right: 0;
		bottom: 0;
		height: min(44vh, 420px);
		pointer-events: none;
		opacity: 0;
		transform: translateY(12px);
		transition:
			opacity var(--m-slow) var(--m-ease),
			transform 1200ms var(--m-ease);
		-webkit-mask-image: linear-gradient(to bottom, transparent, #000 75%);
		mask-image: linear-gradient(to bottom, transparent, #000 75%);
	}
	.setup-print.shown {
		opacity: 0.3;
		transform: none;
	}
	.setup-print.shown.dark {
		opacity: 0.16;
	}
	.setup-print .ink {
		width: 100%;
		height: 100%;
		background: var(--color-primary);
		-webkit-mask: url("/onboarding/tuscany.webp") 58% 100% / cover no-repeat;
		mask: url("/onboarding/tuscany.webp") 58% 100% / cover no-repeat;
	}

	/* Each step's page is a sheet set on the desk: the paper a little
	   lifted, a hairline edge and a long soft shadow. Steps that draw their
	   own room (Names, a map) stay on the desk itself. */
	.flow :global(.frame:not(.bleed)) {
		width: calc(100% - 32px);
		max-width: 42rem;
		min-height: 0;
		box-sizing: border-box;
		margin: clamp(1.5rem, 5vh, 3.5rem) auto 6rem;
		padding: clamp(2rem, 5vw, 3.25rem) clamp(1.25rem, 5vw, 3.5rem) clamp(1.75rem, 4vw, 2.75rem);
		border-radius: var(--radius-lg);
		background: var(--color-surface-overlay, var(--color-surface));
		outline: 1px solid color-mix(in srgb, var(--color-foreground) 9%, transparent);
		outline-offset: -1px;
		/* design-ok: a Setup sheet is paper set on the desk, not a card in the pane (the locked "Final" design). */
		box-shadow:
			0 1px 2px color-mix(in srgb, var(--color-foreground) 4%, transparent),
			0 24px 52px -32px color-mix(in srgb, var(--color-foreground) 32%, transparent);
		animation: sheet-in 700ms var(--m-spring) both;
	}
	@keyframes sheet-in {
		from {
			opacity: 0;
			transform: translateY(18px);
		}
	}
	/* The sentence under a step's title, in the letter's serif and near the
	   text color: on the sheet it is the voice of the page, not a caption. */
	.flow :global(.frame:not(.bleed) .subtitle) {
		max-width: 32rem;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.2rem;
		line-height: 1.5;
		color: color-mix(in srgb, var(--color-foreground) 78%, transparent);
	}
	/* On the sheet the way forward sits closer to the work: the sheet's own
	   edge already sets it apart from the window. */
	.flow :global(.frame:not(.bleed) .actions) {
		margin-top: 1.5rem;
	}

	/* ── arrival ─────────────────────────────────────────────────────── */
	/* The painting blooms out of the ink: from no color to its own, once. */
	.arrival-scene {
		position: fixed;
		z-index: 55;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
		object-position: 72% 100%;
		pointer-events: none;
		animation: bloom 1400ms var(--m-ease) 250ms both;
	}
	@keyframes bloom {
		from {
			opacity: 0;
			filter: saturate(0) contrast(1.1);
		}
		to {
			opacity: 1;
			filter: none;
		}
	}
	/* The sky is where the words go: a veil of the paper over its top half,
	   so the greeting reads on every theme. */
	.arrival-veil {
		position: fixed;
		z-index: 56;
		inset: 0 0 auto 0;
		height: 64vh;
		pointer-events: none;
		background: linear-gradient(
			to bottom,
			color-mix(in srgb, var(--color-surface) 82%, transparent),
			color-mix(in srgb, var(--color-surface) 50%, transparent) 60%,
			transparent
		);
		animation: veil-in 900ms var(--m-ease) both;
	}
	@keyframes veil-in {
		from {
			opacity: 0;
		}
	}
	.arrival {
		position: fixed;
		z-index: 60;
		left: 16px;
		right: 16px;
		/* Under the ∴, which closes at 2.6x its 26px (SetupMark). */
		top: calc(clamp(96px, 24vh, 220px) + 64px);
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
	}
	.arrival-title {
		margin: 0;
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		font-size: clamp(2.6rem, 6.5vw, 4.75rem);
		line-height: 1.02;
		letter-spacing: -0.018em;
		text-wrap: balance;
		color: var(--color-foreground);
	}
	.arrival-line {
		max-width: 34rem;
		margin: 18px 0 0;
		font-family: var(--font-serif, Georgia, serif);
		font-size: clamp(1.15rem, 2vw, 1.35rem);
		line-height: 1.5;
		text-wrap: balance;
		color: var(--color-foreground-muted);
	}
	.arrival-ways {
		margin-top: 32px;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: center;
		gap: 14px 24px;
	}
	.arrival-ways .setup-go {
		padding: 0.85rem 1.9rem;
		font-size: 16px;
		box-shadow: 0 10px 24px -12px color-mix(in srgb, var(--color-primary) 70%, transparent); /* design-ok: Arrival's one way in, lifted off the painting */
	}
	.arrival-ways .setup-past {
		color: var(--color-foreground);
	}
	/* A phone: the way in sits at the foot, in thumb's reach, over the
	   lake; the greeting keeps the sky. */
	@media (max-width: 560px) and (min-height: 521px) {
		.arrival-ways {
			position: fixed;
			left: 20px;
			right: 20px;
			bottom: max(28px, calc(env(safe-area-inset-bottom) + 12px));
			flex-direction: column;
			gap: 16px;
		}
		.arrival-ways .setup-go {
			width: 100%;
			justify-content: center;
			padding: 1rem 1.5rem;
		}
		.arrival-ways .setup-past {
			/* design-ok: over the painting, which is the same on every theme */
			color: white;
			/* design-ok: a dark shadow on every theme, since the painting is the same on every theme */
			text-shadow: 0 1px 10px color-mix(in srgb, black 55%, transparent);
		}
	}

	/* What the assistant has started reading, at the foot of the window:
	   the ∴ breathing beside it, so the work shows while setup goes on. */
	.reading {
		position: fixed;
		z-index: 20;
		left: 50%;
		bottom: max(20px, env(safe-area-inset-bottom));
		transform: translateX(-50%);
		display: flex;
		align-items: center;
		gap: 8px;
		max-width: min(calc(100% - 32px), 28rem);
		margin: 0;
		padding: 8px 16px 8px 12px;
		border-radius: var(--radius-lg);
		background: var(--color-surface-overlay, var(--color-surface));
		outline: 1px solid color-mix(in srgb, var(--color-foreground) 9%, transparent);
		outline-offset: -1px;
		font-size: 13px;
		line-height: 1.4;
		color: var(--color-foreground);
		text-wrap: balance;
	}
	.reading svg {
		flex: none;
		fill: var(--color-primary);
	}
	.reading circle {
		animation: reading 1.8s ease-in-out infinite;
	}
	.reading circle:nth-child(2) {
		animation-delay: 0.3s;
	}
	.reading circle:nth-child(3) {
		animation-delay: 0.6s;
	}
	@keyframes reading {
		0%,
		100% {
			opacity: 0.35;
		}
		50% {
			opacity: 1;
		}
	}

	/* A short window (a phone on its side): everything moves up. */
	@media (max-height: 520px) {
		.arrival {
			top: 112px;
		}
		.arrival-ways {
			margin-top: 18px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.flow {
			transition: none;
		}
		.setup-print {
			transition: none;
			transform: none;
		}
		.flow :global(.frame:not(.bleed)) {
			animation: none;
		}
		.arrival-scene {
			animation: fade-only 600ms ease both;
		}
		.arrival-veil {
			animation: none;
		}
		.reading circle {
			animation: none;
			opacity: 1;
		}
	}
	@keyframes fade-only {
		from {
			opacity: 0;
		}
	}
</style>
