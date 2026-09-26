<!--
  /setup — Setup, the one flow (agents/plan/setup-plan.md).

  THE STAGE (2026-09-25). Paper and grain are painted once, here
  (`.setup-stage`, setup.css), and every step changes on top of them: steps
  crossfade over each other rather than fading through a blank page. The
  progress is the mark itself (SetupMark), which Welcome's big ∴ flies up
  to become. The four laws are in agents/plan/setup-plan.md.

  Nine steps, a dot each: Welcome (the cold open, with light or dark), the
  founder's letter, Server, Wi-Fi, Subscription, Names, Connections,
  Timeline, Interview. Everything through Names is required. The last three
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
	import { goto } from "$app/navigation";
	import { page } from "$app/state";
	import { onMount } from "svelte";
	import { fade } from "svelte/transition";
	import "$lib/components/setup/setup.css";
	import { M } from "$lib/components/setup/motion";
	import { isMacOS } from "$lib/utils/platform";
	import Hello from "$lib/components/onboarding/Hello.svelte";
	import FoundersLetter from "$lib/components/onboarding/document/FoundersLetter.svelte";
	import SetupMark from "$lib/components/setup/SetupMark.svelte";
	import ThemeSwitch from "$lib/components/setup/ThemeSwitch.svelte";
	import { setup, isSetupStep, intoLabel, SERVER, type SetupStepId } from "$lib/components/setup/setup.svelte";
	import { score } from "$lib/components/setup/score.svelte";
	import StepPaired from "$lib/components/setup/steps/StepPaired.svelte";
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

	onMount(() => {
		void (async () => {
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
		try {
			const s = await getSetupState();
			// `active` is what the gate lets through, whatever else is true.
			if (s.onboarding_status !== "active") await skipOnboarding(true);
		} catch {
			// The gate asks again next launch. Annoying beats trapped.
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
		<p class="err" in:fade>Couldn't reach your server. Make sure you're on the same network, then reload this page.</p>
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

	{#if current?.optional && !closing}
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
					{:else if step === "server" || step === "wifi"}
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
		padding: 0 16px;
	}
	.err {
		max-width: 28rem;
		font-size: 14px;
		color: var(--color-error);
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
		top: max(16px, env(safe-area-inset-top));
		right: 16px;
		display: flex;
		gap: 0;
	}
	/* On a phone the nine dots take most of the top line; the corner
	   tightens so it clears them down to a 360px screen. */
	@media (max-width: 440px) {
		.corner {
			right: 8px;
			gap: 0;
		}
		.corner :global(.flip),
		.corner .sound {
			width: 30px;
			height: 30px;
		}
	}
	.sound {
		display: grid;
		place-content: center;
		width: 34px;
		height: 34px;
		border: none;
		border-radius: 50%;
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
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
