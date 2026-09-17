<script lang="ts">
	import { onMount } from "svelte";
	import { updated } from "$app/state";
	import { appUpdateState, applyAppUpdate } from "$lib/tauri/bridge";
	import { onBoxBuildChanged } from "$lib/build";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let {
		collapsed = false,
		animationDelay = 0,
	}: Props = $props();

	// The console line: the rail's only facts, set in mono on a hairline.
	//
	// Not a progress bar — a bar dramatizes a number nobody asked for. This is
	// a colophon: the date, and the time. (Day-of-year was here once and read
	// as trivia — "DAY 210" tells you nothing you wanted to know.)
	let stamp = $state("");
	let clock = $state("");

	// Click the clock to swap 24h ⇄ 12h. Persisted, because a clock that
	// forgets which face you chose is worse than one that never offered.
	let hour12 = $state(false);

	const DAYS = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
	const MONTHS = [
		"JAN", "FEB", "MAR", "APR", "MAY", "JUN",
		"JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
	];

	function tick() {
		const now = new Date();
		stamp = `${DAYS[now.getDay()]} ${MONTHS[now.getMonth()]} ${now.getDate()}`;
		const h = now.getHours();
		const mm = String(now.getMinutes()).padStart(2, "0");
		clock = hour12
			? `${((h + 11) % 12) + 1}:${mm} ${h < 12 ? "AM" : "PM"}`
			: `${String(h).padStart(2, "0")}:${mm}`;
	}

	function toggleClock() {
		hour12 = !hour12;
		try {
			localStorage.setItem("virtues:clock12", hour12 ? "1" : "0");
		} catch {
			// Private mode / storage disabled — the toggle still works this session.
		}
		tick();
	}

	// A staged app update, waiting for a relaunch. The shell stages silently
	// (the Chrome model) and applies on the next launch — but this app is a
	// menu-bar resident designed never to relaunch, so "next launch" rounds to
	// never and the only surface that knew was the tray. This chip is the
	// in-app door: quiet, standing, one click. Null everywhere the
	// self-updater doesn't exist (browser, phone, Windows/Linux), so it simply
	// never renders there.
	let stagedVersion = $state<string | null>(null);

	// ── The OTHER update track ───────────────────────────────────────────────
	//
	// Two clocks reach this app, and until now the chip only knew one. The
	// SHELL stages a signed release and needs a relaunch (above). The UI is a
	// separate thing entirely: the desktop bakes no SPA at all
	// (`frontendDist: "ui"` is the airlock), so after pairing it renders the
	// box's live SPA over the loopback proxy — meaning a box upgrade changes
	// the UI with no app release involved, and this window keeps showing the
	// old bundle until something reloads it. A menu-bar resident is never
	// relaunched, so "until something reloads it" rounds to never, exactly as
	// the shell's own comment says of "next launch".
	//
	// THE SIGNAL IS BUNDLE-TO-BUNDLE, and it took a pinned-open chip to learn
	// why it has to be. This used to compare the SPA's baked commit against
	// `/health`'s — but `/health` reports the BOX BINARY's commit, and the two
	// are different artifacts that the product deliberately lets drift:
	// `virtues upgrade --only web` refreshes the served build with no binary
	// swap ("no migration, no restart", cli/upgrade.rs), and a hand-built box
	// carries a binary from one commit beside a CI-built dist from another.
	// So the comparison was wrong in both directions at once — pinned open
	// whenever the binary was merely DIFFERENT (and `reload` could never
	// change the binary, so the chip came straight back, forever), and silent
	// after a web-only refresh, which is the one case a reload would have
	// fixed. The old comment called the difference "the signal"; it was two
	// questions wearing one name.
	//
	// SvelteKit already answers the real question. It bakes a build id into
	// this bundle and publishes the deployed one at `/_app/version.json`;
	// `updated.current` is the comparison. Same kind on both sides, so it
	// converges by construction: reload, and the page that comes back IS the
	// deployed build. No `dev` special case is needed either — a vite dev
	// server has no version.json to disagree with.
	const uiStale = $derived(updated.current);

	// The dist's own short sha, for the chip's second line. Read from
	// `/api/web-bundle/version`, which describes the build the box is SERVING
	// — the only identity that says anything about the thing being offered.
	// Fetched only when the chip is about to show, because that is the only
	// time anyone reads it.
	let distSha = $state<string | null>(null);

	async function pollUpdate() {
		const s = await appUpdateState();
		stagedVersion = s?.stagedVersion ?? null;

		// No try/catch: `check()` answers `false` on every failure path rather
		// than throwing — non-2xx, unparseable, offline — so a dropped poll or a
		// box mid-upgrade cannot flap the chip, and a box with no version.json
		// at all (headless, or a dev box serving from vite) cannot pin it open.
		// It is also hardwired to `false` in a dev build, which is what the old
		// hand-rolled `!== "dev"` guard was for.
		await updated.check();
		if (!updated.current) return;
		try {
			const res = await fetch("/api/web-bundle/version", { cache: "no-store" });
			// 404 is an ordinary answer here (a dev box serving from vite, a
			// headless install): the chip simply shows no identity line.
			distSha = res.ok ? ((await res.json())?.sha ?? null) : null;
		} catch {
			distSha = null;
		}
	}

	/** Relaunch into the staged release, or take the chip down if there is none. */
	async function relaunch() {
		if (await applyAppUpdate()) return; // unreachable: the app restarts
		// The shell had nothing staged after all. Rather than leave a button
		// that does nothing when pressed, drop the chip and re-read the state
		// that produced it.
		stagedVersion = null;
		void pollUpdate();
	}

	onMount(() => {
		try {
			hour12 = localStorage.getItem("virtues:clock12") === "1";
		} catch {
			// Fall through to the 24h default.
		}
		tick();
		void pollUpdate();
		const id = setInterval(tick, 30_000);
		// The shell checks every 6h; ten minutes keeps the chip honest without
		// chatter on an IPC call that answers from memory.
		const updateId = setInterval(pollUpdate, 600_000);
		// A box that restarts under us is worth knowing about before the next
		// tick of that timer — ten minutes of a page quietly holding chunks the
		// box no longer has is ten minutes too many. See $lib/build.
		const offBoxMoved = onBoxBuildChanged(() => void pollUpdate());
		return () => {
			clearInterval(id);
			clearInterval(updateId);
			offBoxMoved();
		};
	});

	// The three doors that used to live here — Sources, Developer, Settings —
	// are rail items now. A door in the footer AND an icon on the rail is the
	// same destination twice on one screen.
	//
	// There is no "Sign Out" — auth is the device's proven iroh key, not a
	// server session; to drop this device use Settings → Devices → Unpair.
</script>

<div
	class="footer"
	class:collapsed
	style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
>
	<!-- TWO tracks, and each gets its own chip when it has something to say.
	     This was one chip with the app update winning outright, on the argument
	     that relaunching also reloads the UI — true, and still the reason it is
	     listed first. What the argument missed is that it only holds if the
	     relaunch HAPPENS. A staged release is set the moment the background
	     updater stages it and clears only when the process restarts, and this is
	     a menu-bar resident designed never to restart — the same premise the
	     chip below is built on. So a staged update parks in that slot for days
	     and silently blanks the other track for the whole time.

	     Neither ever acts on its own: an update that interrupts what you were
	     typing is worse than an update that waits. -->
	{#if stagedVersion && !collapsed}
		<button
			type="button"
			class="relaunch"
			onclick={() => void relaunch()}
			title="Restart into the downloaded update — takes a few seconds"
		>
			<span class="relaunch-label">Relaunch to update</span>
			<span class="relaunch-version">v{stagedVersion}</span>
		</button>
	{/if}
	{#if uiStale && !collapsed}
		<button
			type="button"
			class="relaunch"
			onclick={() => window.location.reload()}
			title="Your server is serving a newer interface — reload to pick it up"
		>
			<span class="relaunch-label">Reload for the latest</span>
			{#if distSha}<span class="relaunch-version">{distSha}</span>{/if}
		</button>
	{/if}


</div>

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	.footer {
		@apply flex flex-col gap-1 py-3 mt-auto;
		padding-left: 8px;
		/* Staggered load animation (initial mount) */
		animation: sidebar-fade-slide-in 200ms var(--sidebar-transition-easing) backwards;
		/* Staggered expand transition - uses --stagger-delay CSS var */
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--sidebar-transition-easing) var(--stagger-delay, 0ms),
			transform 200ms var(--sidebar-transition-easing) var(--stagger-delay, 0ms);
	}

	/* The update chip: a standing offer, not an alarm. Info tokens (note-this),
	   never the accent (act-on-this is the doors' register) — the split the
	   theme pass argued for. Reads like a Library row that happens to carry a
	   second, dim line. */
	.relaunch {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 1px;
		width: 100%;
		padding: 6px var(--sidebar-interactive-padding);
		margin-bottom: 4px;
		border: 1px solid var(--color-info-subtle);
		border-radius: var(--sidebar-interactive-radius);
		background: var(--color-info-subtle);
		cursor: pointer;
		text-align: left;
	}

	.relaunch-label {
		font-size: var(--sidebar-interactive-font-size);
		color: var(--color-info);
	}

	.relaunch-version {
		font-family: var(--font-mono);
		font-size: 9.5px;
		letter-spacing: 0.07em;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}

	.relaunch:hover {
		border-color: var(--color-info);
	}

	.relaunch:focus-visible {
		outline: 2px solid var(--color-info);
		outline-offset: -2px;
	}

	.door {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: var(--sidebar-interactive-padding);
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: var(--sidebar-interactive-font-size);
		color: var(--sidebar-interactive-color);
	}

	.door :global(.atlas-icon) {
		color: var(--color-foreground-muted);
		opacity: var(--sidebar-icon-opacity);
	}

	.door:hover :global(.atlas-icon) {
		opacity: 1;
	}

	.door.collapsed {
		justify-content: center;
		gap: 0;
	}

	/* The library card + the colophon. Doors read exactly like Library rows —
	   separation is distance, not a second type register. */
	.console {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 8px;
		margin: 6px 0 0;
		padding: 8px 10px 0 var(--sidebar-padding-left-base);
		border-top: 1px solid var(--color-border-subtle);
		font-family: var(--font-mono);
		font-size: 9.5px;
		letter-spacing: 0.07em;
		color: var(--color-foreground-disabled);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		user-select: none;
	}

	/* A button for the affordance, but it must read as the same piece of text
	   the span was — so inherit the console's type and drop every default. */
	.console-clock {
		appearance: none;
		background: none;
		border: 0;
		padding: 0;
		margin: 0;
		font: inherit;
		letter-spacing: inherit;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}

	.console-clock:hover {
		color: var(--color-foreground);
	}

	.console-clock:focus-visible {
		outline: 1px solid var(--color-border);
		outline-offset: 2px;
	}

	.door:hover {
		background: var(--sidebar-hover-bg);
	}

	.door.active {
		background: var(--sidebar-active-bg);
	}

	.door:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.footer.collapsed {
		@apply items-center;
		padding-left: 4px;
		padding-right: 4px;
		opacity: 0;
		transition:
			opacity var(--sidebar-transition-duration) var(--sidebar-transition-easing),
			transform var(--sidebar-transition-duration) var(--sidebar-transition-easing);
	}
</style>
