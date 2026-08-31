<!--
  /onboarding — the founder's letter, and nothing else.

  WAS A SEQUENCE, IS NOW A THRESHOLD (2026-08-31). The four-step flow —
  letter, introductions, sources, reveal — asked its questions at the one
  moment the box had nothing to show for them: nothing kicks when a source
  connects, entities resolve on a 15-minute tick, and the first narrated day
  lands the following morning. So the payoff of connecting a life could never
  be delivered here, only promised. Everything except the letter moved into
  the app as Home's getting-started sections, which retire individually as
  they are answered or as their promises land — see
  agents/plan/getting-started-plan.md, and GettingStarted.svelte for the
  sections themselves.

  What stays is the one thing that must be read before the app and cannot
  retire: the letter. It sets the covenant; the button at its end is the door.

  THE URL SPACE SHRANK WITH IT. `[[view]]` remains only so that old step URLs
  (/onboarding/introductions, /sources, /you) land here instead of 404ing —
  they normalize to /onboarding with replaceState, since a correction is not
  somewhere the person navigated to.

  THE ACCOUNT GATE MOVED TOO. It was a toll booth on the reveal (the one
  onboarding surface that called the models); with the reveal gone, the toll
  booth stands where the models are actually called — Home's getting-started
  renders AccountGate while the account is unsatisfied.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import { page } from "$app/state";
	import { onMount } from "svelte";
	import { fade } from "svelte/transition";
	import Icon from "$lib/components/Icon.svelte";
	import { getProfile, updateProfile, getSetupState, skipOnboarding } from "$lib/api/client";
	import FoundersLetter from "$lib/components/onboarding/document/FoundersLetter.svelte";

	type SetupState = {
		setup_complete: boolean;
		onboarding_complete: boolean;
		onboarding_status: string;
	};

	let state_ = $state<SetupState | null>(null);
	let loading = $state(true);
	let reduced = $state(false);

	async function refreshState() {
		try {
			state_ = await getSetupState();
		} catch {
			/* box briefly unreachable — keep last state */
		} finally {
			loading = false;
		}
	}

	// Old step URLs normalize to the letter. `replaceState` — a correction is
	// not somewhere they navigated to, and leaving it in history would make
	// Back bounce off it forever.
	$effect(() => {
		if (page.params.view) {
			void goto("/onboarding", { replaceState: true });
		}
	});

	// Cloud/onboarding cross-check for home_timezone (the box's location).
	// The box normally seeds this from its own system clock; but a datacenter box
	// reads "UTC", which is wrong. So only fall back to this browser's zone when
	// the server value is unset or UTC — a real appliance configured at home
	// keeps its server-detected zone. See agents/record/timezone-model.md.
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
		void captureTimezone();
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
</script>

{#if loading}
	<div class="flex min-h-screen items-center justify-center gap-2.5 text-sm text-foreground-muted" in:fade>
		<Icon icon="ri:loader-4-line" class="animate-spin" />
		<span>Checking your server…</span>
	</div>
{:else if !state_}
	<div class="flex min-h-screen items-center justify-center px-6">
		<div class="rounded-xl border border-error/20 bg-error-subtle p-4 text-sm text-error" in:fade>
			Couldn't reach the box. Make sure you're on the same network, then refresh.
		</div>
	</div>
{:else}
	<div class="ob-wrap" class:ob-still={reduced}>
		<div class="ob-sheet">
			<div class="ob-page">
				<FoundersLetter onbegin={enterApp} />
			</div>
		</div>
	</div>
{/if}

<style>
	/* Four levels, not three — this file sits one deeper than it used to, under
	   `[[view]]`. svelte-check does not resolve `@reference`, so a stale path
	   here typechecks clean and 500s only the style request, which the browser
	   reports as "failed to fetch dynamically imported module" for the whole
	   page. Blank screen, no error naming this line. */
	@reference "../../../../app.css";

	/* The shell, type scale and controls come from onboarding.css — see that
	   file for why they are not here. */
</style>
