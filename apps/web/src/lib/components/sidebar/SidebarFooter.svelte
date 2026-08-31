<script lang="ts">
	import { onMount } from "svelte";
	import { sidebarMode } from "$lib/stores/sidebarMode.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { appUpdateState, applyAppUpdate } from "$lib/tauri/bridge";
	import AtlasIcon from "./AtlasIcon.svelte";

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
	// `/health` already reports the box's commit, and the running bundle knows
	// its own, so the difference IS the signal — no new endpoint, no version
	// negotiation. `dev` is skipped because a dev SPA and a dev binary are
	// built separately and always differ, which would pin the chip open.
	// @ts-ignore — Vite compile-time constant (see vite.config.ts + app.d.ts)
	const BUILD_COMMIT: string = __BUILD_COMMIT__;
	let boxCommit = $state<string | null>(null);
	const uiStale = $derived(
		BUILD_COMMIT !== "dev" && !!boxCommit && boxCommit !== BUILD_COMMIT
	);

	async function pollUpdate() {
		const s = await appUpdateState();
		stagedVersion = s?.stagedVersion ?? null;

		// Cheap and unauthenticated; served by the box we're already rendering.
		try {
			const res = await fetch("/health", { cache: "no-store" });
			if (res.ok) {
				const h = await res.json();
				boxCommit = typeof h?.commit === "string" ? h.commit : null;
			}
		} catch {
			// Offline or mid-upgrade — keep the last verdict rather than
			// flapping the chip on a dropped poll.
		}
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
		return () => {
			clearInterval(id);
			clearInterval(updateId);
		};
	});

	// Three doors, each of which swaps the sidebar into its own mode rather than
	// navigating anywhere directly — see lib/sidebar/modes.ts. Developer is its
	// own door instead of a section inside Settings, which is what let Settings
	// drop the second row of underline tabs it had grown; Sources left Settings
	// for the same reason, having been one row between Assistant and Billing.
	//
	// Ordered by how often you mean it: Sources answers "is my data still
	// arriving", which is a question worth asking far more often than either of
	// the other two.
	//
	// There is no "Sign Out" — auth is the device's proven iroh key, not a
	// server session; to drop this device use Settings → Devices → Unpair.
	// `href` opens the room's front page as well as swapping the rail. Settings
	// and Developer deliberately don't: their first row is a preference screen
	// you may not have come for, and swapping the rail under a pane you were
	// reading is the cheaper move. Sources is the opposite — Overview *is* the
	// answer to why you opened the door ("is my data still arriving"), so making
	// you click twice for it would be the wrong default.
	const doors = [
		{ id: "sources", label: "Sources", icon: "sources", href: "/sources" },
		{ id: "developer", label: "Developer", icon: "developer", href: null },
		{ id: "settings", label: "Settings", icon: "settings", href: null },
	];

	function openDoor(door: (typeof doors)[number]) {
		sidebarMode.enter(door.id);
		if (door.href) {
			windowShellStore.navigate(door.href, { label: door.label });
		}
	}
</script>

<div
	class="footer"
	class:collapsed
	style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
>
	<!-- ONE chip, two tracks — never both at once. A staged shell release wins
	     because relaunching also reloads the UI, so offering "Reload" beside it
	     would be offering the smaller half of what the other button already
	     does. Neither ever acts on its own: an update that interrupts what you
	     were typing is worse than an update that waits. -->
	{#if stagedVersion && !collapsed}
		<button
			type="button"
			class="relaunch"
			onclick={() => void applyAppUpdate()}
			title="Restart into the downloaded update — takes a few seconds"
		>
			<span class="relaunch-label">Relaunch to update</span>
			<span class="relaunch-version">v{stagedVersion}</span>
		</button>
	{:else if uiStale && !collapsed}
		<button
			type="button"
			class="relaunch"
			onclick={() => window.location.reload()}
			title="Your server is serving a newer interface — reload to pick it up"
		>
			<span class="relaunch-label">Reload for the latest</span>
			<span class="relaunch-version">{boxCommit?.slice(0, 7)}</span>
		</button>
	{/if}

	{#each doors as door (door.id)}
		<button
			type="button"
			class="door"
			class:collapsed
			class:active={sidebarMode.activeId === door.id}
			onclick={() => openDoor(door)}
			title={door.label}
		>
			<AtlasIcon name={door.icon} />
			{#if !collapsed}<span>{door.label}</span>{/if}
		</button>
	{/each}

	{#if !collapsed}
		<div class="console">
			<span>{stamp}</span>
			<button
				type="button"
				class="console-clock"
				onclick={toggleClock}
				title={hour12 ? "Switch to 24-hour" : "Switch to 12-hour"}
				aria-label={`Time ${clock}. Switch to ${hour12 ? "24" : "12"}-hour clock.`}
			>{clock}</button>
		</div>
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
